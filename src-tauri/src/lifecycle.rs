//! Serialized owner for exactly one Codex process tree.
//! A stop is not completion: reconnect is prohibited until shutdown is confirmed.
use tokio::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateError {
    Busy,
    Poisoned,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StartError<E> {
    Gate(GateError),
    Start(E),
}

enum Slot<T> {
    Empty,
    Running { generation: u64, handle: T },
    Stopping { generation: u64 },
    Poisoned,
}

pub struct SessionGate<T> {
    slot: Mutex<Slot<T>>,
}

impl<T: Clone> Default for SessionGate<T> {
    fn default() -> Self {
        Self { slot: Mutex::new(Slot::Empty) }
    }
}

impl<T: Clone> SessionGate<T> {
    /// The factory runs with the gate locked, so two contenders cannot spawn
    /// two native children before one notices the resource lock.
    pub async fn start<R, E>(
        &self,
        generation: u64,
        factory: impl FnOnce() -> Result<(T, R), E>,
    ) -> Result<R, StartError<E>> {
        let mut slot = self.slot.lock().await;
        match &*slot {
            Slot::Empty => {}
            Slot::Poisoned => return Err(StartError::Gate(GateError::Poisoned)),
            _ => return Err(StartError::Gate(GateError::Busy)),
        };
        let (handle, runtime) = factory().map_err(StartError::Start)?;
        *slot = Slot::Running { generation, handle };
        Ok(runtime)
    }

    pub async fn current(&self) -> Option<(u64, T)> {
        match &*self.slot.lock().await {
            Slot::Running { generation, handle } => Some((*generation, handle.clone())),
            _ => None,
        }
    }

    /// The Stopping sentinel stays installed across all async teardown work.
    pub async fn begin_stop(&self) -> Result<Option<(u64, T)>, GateError> {
        let mut slot = self.slot.lock().await;
        match &*slot {
            Slot::Empty => return Ok(None),
            Slot::Poisoned => return Err(GateError::Poisoned),
            Slot::Stopping { .. } => return Err(GateError::Busy),
            Slot::Running { .. } => {}
        }
        if let Slot::Running { generation, handle } =
            std::mem::replace(&mut *slot, Slot::Poisoned)
        {
            *slot = Slot::Stopping { generation };
            Ok(Some((generation, handle)))
        } else {
            unreachable!("running state checked while holding lock")
        }
    }

    /// Return false when an obsolete actor attempts to change a newer slot.
    pub async fn finish_stop(&self, generation: u64, clean: bool) -> bool {
        let mut slot = self.slot.lock().await;
        if matches!(&*slot, Slot::Stopping { generation: current } if *current == generation) {
            *slot = if clean { Slot::Empty } else { Slot::Poisoned };
            true
        } else {
            false
        }
    }

    /// Natural crash/EOF path. Never free an active stop sentinel here.
    pub async fn finish_unexpected(&self, generation: u64, clean: bool) -> bool {
        let mut slot = self.slot.lock().await;
        if matches!(&*slot, Slot::Running { generation: current, .. } if *current == generation) {
            *slot = if clean { Slot::Empty } else { Slot::Poisoned };
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn rapid_reconnect_waits_for_confirmed_shutdown() {
        let gate = Arc::new(SessionGate::<usize>::default());
        let alive = Arc::new(AtomicUsize::new(0));
        for generation in 1..=20 {
            gate.start(generation, || {
                let previous = alive.fetch_add(1, Ordering::SeqCst);
                assert_eq!(previous, 0, "overlapping process trees");
                Ok::<_, ()>((generation as usize, ()))
            }).await.unwrap();
            let (id, _) = gate.begin_stop().await.unwrap().unwrap();
            let (release, stopped) = oneshot::channel::<()>();
            let gate_clone = gate.clone();
            let alive_clone = alive.clone();
            let finishing = tokio::spawn(async move {
                stopped.await.unwrap();
                alive_clone.fetch_sub(1, Ordering::SeqCst);
                gate_clone.finish_stop(id, true).await
            });
            assert_eq!(
                gate.start(generation + 1000, || -> Result<(usize, ()), ()> {
                    panic!("must not spawn before stop has completed")
                }).await,
                Err(StartError::Gate(GateError::Busy))
            );
            release.send(()).unwrap();
            assert!(finishing.await.unwrap());
        }
        assert_eq!(alive.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn simultaneous_connects_spawn_exactly_once() {
        let gate = Arc::new(SessionGate::<usize>::default());
        let spawned = Arc::new(AtomicUsize::new(0));
        let mut workers = Vec::new();
        for generation in 1..=24 {
            let gate = gate.clone();
            let spawned = spawned.clone();
            workers.push(tokio::spawn(async move {
                gate.start(generation, || {
                    spawned.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, ()>((generation as usize, ()))
                }).await.is_ok()
            }));
        }
        let mut successes = 0;
        for worker in workers { successes += usize::from(worker.await.unwrap()); }
        assert_eq!(successes, 1);
        assert_eq!(spawned.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_stop_poisoned_and_cannot_reconnect() {
        let gate = SessionGate::<usize>::default();
        gate.start(1, || Ok::<_, ()>((12, ()))).await.unwrap();
        let (generation, _) = gate.begin_stop().await.unwrap().unwrap();
        assert!(gate.finish_stop(generation, false).await);
        assert!(gate.current().await.is_none());
        assert_eq!(
            gate.start(2, || -> Result<(usize, ()), ()> { panic!("must fail closed") }).await,
            Err(StartError::Gate(GateError::Poisoned))
        );
    }

    #[tokio::test]
    async fn old_process_exit_cannot_erase_new_generation() {
        let gate = SessionGate::<usize>::default();
        gate.start(1, || Ok::<_, ()>((1, ()))).await.unwrap();
        assert!(gate.finish_unexpected(1, true).await);
        gate.start(2, || Ok::<_, ()>((2, ()))).await.unwrap();
        assert!(!gate.finish_unexpected(1, true).await);
        assert!(!gate.finish_stop(1, true).await);
        assert_eq!(gate.current().await, Some((2, 2)));
        assert!(gate.finish_unexpected(2, true).await);
    }

    #[tokio::test]
    async fn spontaneous_failed_teardown_stays_poisoned() {
        let gate = SessionGate::<usize>::default();
        gate.start(1, || Ok::<_, ()>((1, ()))).await.unwrap();
        assert!(gate.finish_unexpected(1, false).await);
        assert_eq!(
            gate.begin_stop().await,
            Err(GateError::Poisoned)
        );
    }
}
