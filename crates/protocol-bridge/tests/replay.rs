use serde_json::{json, Value};
use ultima_protocol_bridge::*;

fn make_ready() -> Broker {
    let mut broker = Broker::new(Limits::default()).unwrap();
    let id = broker.begin_initialize().unwrap();
    assert_eq!(id, 0);
    assert_eq!(broker.phase(), Phase::Initializing);
    assert!(broker.call(ClientMethod::ModelList, json!({})).is_err());
    let request: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
    assert_eq!(request["params"]["capabilities"]["experimentalApi"], false);
    broker.ingest_line(br#"{"id":0,"result":{"userAgent":"codex/0.164.0","platformFamily":"unix","platformOs":"linux"}}"#).unwrap();
    assert_eq!(broker.phase(), Phase::Acknowledgeable);
    broker.acknowledge_initialized().unwrap();
    let ack: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
    assert_eq!(ack["method"], "initialized");
    assert_eq!(broker.phase(), Phase::Ready);
    broker.take_event().unwrap();
    broker
}
#[test]
fn handshake_is_ordered_and_id_monotonic() {
    let mut b = make_ready();
    let a = b
        .call(ClientMethod::ModelList, json!({"limit":10}))
        .unwrap();
    let c = b
        .call(ClientMethod::ThreadResume, json!({"threadId":"abc"}))
        .unwrap();
    assert_eq!((a, c), (1, 2));
    b.ingest_line(br#"{"id":2,"result":{"thread":{"id":"abc"}}}"#)
        .unwrap();
    b.ingest_line(br#"{"id":1,"error":{"code":-32000,"message":"oops"}}"#)
        .unwrap();
    assert!(matches!(b.take_event(), Some(Event::Reply { id: 2, .. })));
    assert!(matches!(
        b.take_event(),
        Some(Event::Reply {
            id: 1,
            outcome: Err(_),
            ..
        })
    ));
    assert_eq!(b.pending_count(), 0);
    b.ingest_line(br#"{"id":1,"result":{}}"#).unwrap();
    assert_eq!(b.event_count(), 0); // duplicate ignored
}
#[test]
fn queued_cancel_removes_unsent_request_and_late_reply_is_ignored() {
    let mut b = make_ready();
    let id = b
        .call(
            ClientMethod::TurnStart,
            json!({"threadId":"t","input":[{"type":"text","text":"hi"}]}),
        )
        .unwrap();
    assert_eq!(b.cancel_local(id).unwrap(), Cancelled::Unsent);
    assert!(b.take_outbound().is_none());
    b.ingest_line(format!(r#"{{"id":{id},"result":{{}}}}"#).as_bytes())
        .unwrap();
    assert!(b.take_event().is_none());
    let id = b
        .call(ClientMethod::TurnStart, json!({"threadId":"t","input":[]}))
        .unwrap();
    b.take_outbound().unwrap();
    assert_eq!(b.cancel_local(id).unwrap(), Cancelled::Detached);
    let interrupt = b.interrupt_turn("t", "turn").unwrap();
    let wire: Value = serde_json::from_slice(&b.take_outbound().unwrap()).unwrap();
    assert_eq!(wire["id"], interrupt);
    assert_eq!(wire["method"], "turn/interrupt");
}
#[test]
fn approval_is_never_implicit_and_unknown_requests_fail_closed() {
    let mut b = make_ready();
    b.ingest_line(br#"{"id":"tool1","method":"item/commandExecution/requestApproval","params":{"threadId":"t","turnId":"z","itemId":"i","startedAtMs":3}}"#).unwrap();
    let event = b.take_event().unwrap();
    assert!(matches!(event, Event::Approval(_)));
    let id = RpcId::String("tool1".into());
    assert_eq!(
        b.reply_approval(&id, ApprovalDecision::Accept, false),
        Err(BridgeError::ExplicitApprovalRequired)
    );
    assert_eq!(b.approval_count(), 1);
    b.decline_approval(&id).unwrap();
    let reply: Value = serde_json::from_slice(&b.take_outbound().unwrap()).unwrap();
    assert_eq!(reply["result"]["decision"], "decline");
    assert_eq!(
        b.reply_approval(&id, ApprovalDecision::Accept, true),
        Err(BridgeError::UnknownApproval)
    );
    b.ingest_line(br#"{"id":99,"method":"future/grantPrivilege","params":{}}"#)
        .unwrap();
    let rejected: Value = serde_json::from_slice(&b.take_outbound().unwrap()).unwrap();
    assert_eq!(rejected["error"]["code"], -32601);
    b.ingest_line(br#"{"id":3,"method":"item/fileChange/requestApproval","params":{"threadId":"","turnId":"z","itemId":"i","startedAtMs":3}}"#).unwrap();
    let invalid: Value = serde_json::from_slice(&b.take_outbound().unwrap()).unwrap();
    assert_eq!(invalid["error"]["code"], -32602);
}
#[test]
fn unknown_notifications_do_not_accumulate_and_known_events_replay() {
    let mut b = make_ready();
    for _ in 0..10_000 {
        b.ingest_line(br#"{"method":"future/event","params":{"blob":"no retention"}}"#)
            .unwrap();
    }
    assert_eq!(b.event_count(), 0);
    b.ingest_line(br#"{"method":"thread/started","params":{"thread":{"id":"t"}}}"#)
        .unwrap();
    b.ingest_line(br#"{"method":"item/agentMessage/delta","params":{"delta":"hi"}}"#)
        .unwrap();
    b.ingest_line(
        br#"{"method":"turn/completed","params":{"turn":{"id":"z","status":"completed"}}}"#,
    )
    .unwrap();
    assert!(matches!(
        b.take_event(),
        Some(Event::Notification {
            kind: NotificationKind::ThreadStarted,
            ..
        })
    ));
    assert!(matches!(
        b.take_event(),
        Some(Event::Notification {
            kind: NotificationKind::AgentMessageDelta,
            ..
        })
    ));
    assert!(matches!(
        b.take_event(),
        Some(Event::Notification {
            kind: NotificationKind::TurnCompleted,
            ..
        })
    ));
    assert_eq!(b.event_bytes(), 0);
}
#[test]
fn bounded_events_pending_approvals_and_outgoing_are_enforced() {
    let limits = Limits {
        events: 1,
        pending: 1,
        approvals: 1,
        outbound: 1,
        ..Limits::default()
    };
    let mut b = Broker::new(limits).unwrap();
    b.begin_initialize().unwrap();
    assert_eq!(b.begin_initialize(), Err(BridgeError::InvalidState));
    b.take_outbound();
    b.ingest_line(br#"{"id":0,"result":{"userAgent":"a","platformFamily":"b","platformOs":"c"}}"#)
        .unwrap();
    assert_eq!(b.event_count(), 1);
    b.acknowledge_initialized().unwrap();
    b.take_outbound();
    b.call(ClientMethod::ModelList, json!({})).unwrap();
    assert_eq!(
        b.call(ClientMethod::ModelList, json!({})),
        Err(BridgeError::Backpressure)
    );
    assert_eq!(b.pending_count(), 1);
    assert_eq!(
        b.ingest_line(br#"{"method":"turn/started","params":{}}"#),
        Err(BridgeError::Backpressure)
    );
    b.take_event();
    b.take_outbound();
    b.ingest_line(br#"{"id":1,"result":{}}"#).unwrap();
    assert_eq!(b.pending_count(), 0);
    b.take_event();
    b.ingest_line(br#"{"id":4,"method":"item/fileChange/requestApproval","params":{"threadId":"t","turnId":"r","itemId":"i","startedAtMs":4}}"#).unwrap();
    assert_eq!(b.ingest_line(br#"{"id":5,"method":"item/fileChange/requestApproval","params":{"threadId":"t","turnId":"r","itemId":"i","startedAtMs":5}}"#), Err(BridgeError::Backpressure));
}
#[test]
fn framing_splits_crlf_rejects_oversize_utf8_and_truncated_tail() {
    let mut decoder = LineDecoder::new(30).unwrap();
    let mut lines = Vec::<Vec<u8>>::new();
    decoder
        .feed(b"{\"id\":0,\r", |line| {
            lines.push(line.to_vec());
            Ok(())
        })
        .unwrap();
    decoder
        .feed(b"\n{\"id\":1}\n", |line| {
            lines.push(line.to_vec());
            Ok(())
        })
        .unwrap();
    assert_eq!(lines, [br#"{"id":0,"#.to_vec(), br#"{"id":1}"#.to_vec()]);
    assert!(decoder.finish().is_ok());
    assert_eq!(
        decoder.feed(&[b'x'; 31], |_| Ok(())),
        Err(BridgeError::LineTooLarge)
    );
    let mut dangling = LineDecoder::new(30).unwrap();
    dangling.feed(b"partial", |_| Ok(())).unwrap();
    assert_eq!(dangling.finish(), Err(BridgeError::TruncatedLine));
    assert_eq!(
        make_ready().ingest_line(&[0xff]),
        Err(BridgeError::InvalidUtf8)
    );
}
#[test]
fn malformed_and_version_drift_fail_safe_and_restart_clears_state() {
    let mut b = make_ready();
    assert_eq!(
        b.call(ClientMethod::ThreadResume, json!({})),
        Err(BridgeError::InvalidRequest)
    );
    assert_eq!(
        b.call(
            ClientMethod::TurnStart,
            json!({"threadId":"x","input":[{"type":"image","url":"file:///"}]})
        ),
        Err(BridgeError::InvalidRequest)
    );
    assert_eq!(b.ingest_line(b"[]"), Err(BridgeError::InvalidEnvelope));
    assert_eq!(
        b.ingest_line(br#"{"id":123,"result":{},"error":{}}"#),
        Err(BridgeError::InvalidEnvelope)
    );
    b.call(ClientMethod::ModelList, json!({})).unwrap();
    b.reset_for_new_process();
    assert_eq!(b.phase(), Phase::Fresh);
    assert_eq!(b.pending_count(), 0);
    assert!(b.take_outbound().is_none());
    assert_eq!(b.begin_initialize().unwrap(), 2);
}

#[test]
fn nc07_exact_identity_replay_and_interrupt_deny() {
    let mut b = make_ready();
    b.ingest_line(br#"{"id":"req","method":"item/commandExecution/requestApproval","params":{"threadId":"thread","turnId":"turn","itemId":"item","startedAtMs":19}}"#).unwrap();
    let Event::Approval(valid) = b.take_event().unwrap() else {
        panic!("expected approval")
    };
    let mut stale = valid.clone();
    stale.started_at_ms = 20;
    assert_eq!(
        b.decide_exact_approval(&stale, ApprovalDecision::Accept, true),
        Err(BridgeError::UnknownApproval)
    );
    assert_eq!(
        b.decide_exact_approval(&valid, ApprovalDecision::Accept, false),
        Err(BridgeError::ExplicitApprovalRequired)
    );
    assert_eq!(b.approval_count(), 1);
    assert_eq!(b.decline_approvals_for_turn("other", "turn").unwrap(), 0);
    assert_eq!(b.decline_approvals_for_turn("thread", "turn").unwrap(), 1);
    let denied: Value = serde_json::from_slice(&b.take_outbound().unwrap()).unwrap();
    assert_eq!(denied["result"]["decision"], "decline");
    assert_eq!(
        b.decide_exact_approval(&valid, ApprovalDecision::Accept, true),
        Err(BridgeError::UnknownApproval)
    );
    assert_eq!(b.decline_approvals_for_turn("thread", "turn").unwrap(), 0);
}
#[test]
fn nc07_command_output_delta_is_known_but_bounded() {
    let mut b = make_ready();
    b.ingest_line(br#"{"method":"item/commandExecution/outputDelta","params":{"threadId":"t","itemId":"item","delta":"line"}} "#.trim_ascii()).unwrap();
    assert!(matches!(
        b.take_event(),
        Some(Event::Notification {
            kind: NotificationKind::CommandOutputDelta,
            ..
        })
    ));
    assert_eq!(b.event_bytes(), 0);
}

#[test]
fn nc07_one_time_explicit_accept_never_replays() {
    let mut b = make_ready();
    b.ingest_line(br#"{"id":"accept1","method":"item/fileChange/requestApproval","params":{"threadId":"t","turnId":"z","itemId":"f","startedAtMs":4}}"#).unwrap();
    let Event::Approval(req) = b.take_event().unwrap() else {
        panic!("expected approval")
    };
    b.decide_exact_approval(&req, ApprovalDecision::Accept, true)
        .unwrap();
    let accepted: Value = serde_json::from_slice(&b.take_outbound().unwrap()).unwrap();
    assert_eq!(accepted["id"], "accept1");
    assert_eq!(accepted["result"]["decision"], "accept");
    assert_eq!(b.approval_count(), 0);
    assert_eq!(
        b.decide_exact_approval(&req, ApprovalDecision::Accept, true),
        Err(BridgeError::UnknownApproval)
    );
    b.reset_for_new_process();
    assert_eq!(
        b.decide_exact_approval(&req, ApprovalDecision::Accept, true),
        Err(BridgeError::UnknownApproval)
    );
}

#[test]
fn nc07_upstream_resolved_invalidates_original_request() {
    let mut b = make_ready();
    b.ingest_line(br#"{"id":"resolved1","method":"item/commandExecution/requestApproval","params":{"threadId":"thread","turnId":"turn","itemId":"item","startedAtMs":19}}"#).unwrap();
    let Event::Approval(valid) = b.take_event().unwrap() else {
        panic!("expected approval")
    };
    b.ingest_line(br#"{"method":"serverRequest/resolved","params":{"requestId":"resolved1"}}"#)
        .unwrap();
    assert!(matches!(
        b.take_event(),
        Some(Event::Notification {
            kind: NotificationKind::ServerRequestResolved,
            ..
        })
    ));
    assert_eq!(b.approval_count(), 0);
    assert_eq!(
        b.decide_exact_approval(&valid, ApprovalDecision::Accept, true),
        Err(BridgeError::UnknownApproval)
    );
}

#[test]
fn descriptor_is_exactly_bound_and_same_id_mutation_is_rejected() {
    let mut broker = make_ready();
    broker
        .ingest_line(
            br#"{"id":"cmd","method":"item/commandExecution/requestApproval",
        "params":{"threadId":"t","turnId":"u","itemId":"i",
            "startedAtMs":1,"command":"echo safe","cwd":"/project",
            "availableDecisions":["accept","decline"]}}"#,
        )
        .unwrap();
    let Event::Approval(original) = broker.take_event().unwrap() else {
        panic!("missing server approval");
    };
    assert_eq!(original.request["command"], "echo safe");
    let mut tampered = original.clone();
    tampered.request["command"] = serde_json::json!("echo evil");
    assert_eq!(
        broker.decide_exact_approval(&tampered, ApprovalDecision::Accept, true),
        Err(BridgeError::UnknownApproval)
    );
    assert_eq!(broker.approval_count(), 1);
    broker.decline_approval(&original.id).unwrap();
    assert_eq!(broker.approval_count(), 0);
}


#[test]
fn one_shot_accept_with_execpolicy_proposal_emits_only_accept_and_does_not_persist() {
    let mut broker = make_ready();
    let first_params = json!({
        "threadId":"t","turnId":"u","itemId":"i1","startedAtMs":7,
        "command":"echo safe","cwd":"/project",
        "proposedExecpolicyAmendment":["echo","safe"],
        "availableDecisions":[
            "accept",
            "acceptForSession",
            {"acceptWithExecpolicyAmendment":{"execpolicy_amendment":["echo","safe"]}}
        ]
    });
    let first = json!({
        "id":"proposal1",
        "method":"item/commandExecution/requestApproval",
        "params":first_params
    });
    broker.ingest_line(&serde_json::to_vec(&first).unwrap()).unwrap();
    let Event::Approval(original) = broker.take_event().unwrap() else {
        panic!("expected proposal-bearing approval");
    };
    assert_eq!(
        original.request["proposedExecpolicyAmendment"],
        json!(["echo","safe"])
    );
    broker
        .decide_exact_approval(&original, ApprovalDecision::Accept, true)
        .unwrap();
    let response: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
    assert_eq!(
        response,
        json!({"id":"proposal1","result":{"decision":"accept"}})
    );
    assert_eq!(broker.approval_count(), 0);

    let second = json!({
        "id":"proposal2",
        "method":"item/commandExecution/requestApproval",
        "params":{
            "threadId":"t","turnId":"u","itemId":"i2","startedAtMs":8,
            "command":"echo safe","cwd":"/project",
            "proposedExecpolicyAmendment":["echo","safe"],
            "availableDecisions":["accept","acceptForSession"]
        }
    });
    broker.ingest_line(&serde_json::to_vec(&second).unwrap()).unwrap();
    assert!(matches!(broker.take_event(), Some(Event::Approval(_))));
    assert_eq!(broker.approval_count(), 1);
    assert!(broker.take_outbound().is_none());
}
