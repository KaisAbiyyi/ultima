//! Test-only executable: exercise actual native, host and broker code together.
#[path = "../../../src-tauri/src/approval_binding.rs"]
pub mod host_binding;
#[path = "../../../experiments/native-renderer/src/native_tools.rs"]
pub mod native_tools;

use serde_json::{json, Value};
use ultima_protocol_bridge::{
    Approval, ApprovalDecision, ApprovalKind, Broker, Event, Limits, NotificationKind,
};

fn pending(request: &Value) -> (Broker, Approval) {
    let mut broker = Broker::new(Limits::default()).unwrap();
    broker.begin_initialize().unwrap();
    broker.take_outbound().unwrap();
    broker.ingest_line(br#"{"id":0,"result":{"userAgent":"parity-fixture","platformFamily":"fixture","platformOs":"fixture"}}"#).unwrap();
    broker.acknowledge_initialized().unwrap();
    broker.take_outbound().unwrap();
    broker.take_event().unwrap();
    broker
        .ingest_line(&serde_json::to_vec(request).unwrap())
        .unwrap();
    let Event::Approval(approval) = broker.take_event().unwrap() else {
        panic!("inspectable request");
    };
    assert_eq!(approval.request, request["params"]);
    (broker, approval)
}

fn dispatch(request: &Value, eligible: bool, direct: bool) -> Value {
    let (mut broker, approval) = pending(request);
    let accepted = eligible
        && if direct {
            broker
                .reply_approval(&approval.id, ApprovalDecision::Accept, true)
                .is_ok()
        } else {
            broker
                .decide_exact_approval(&approval, ApprovalDecision::Accept, true)
                .is_ok()
        };
    if !accepted {
        assert!(broker.take_outbound().is_none());
        assert_eq!(broker.approval(&approval.id), Some(&approval));
        broker.decline_approval(&approval.id).unwrap();
    }
    let wire: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
    assert_eq!(broker.approval_count(), 0);
    json!({"attemptedAccept":eligible,"accepted":accepted,"wire":wire})
}

fn main() {
    let command: Value =
        serde_json::from_str(include_str!("../../fixtures/nc10g-local-approval.json")).unwrap();
    let file: Value = serde_json::from_str(include_str!(
        "../../fixtures/nc10g-filechange-approval.json"
    ))
    .unwrap();
    let cases: Value =
        serde_json::from_str(include_str!("../../fixtures/nc10g-environments.json")).unwrap();
    let project = std::env::current_dir().unwrap().canonicalize().unwrap();
    let cwd = project.to_str().unwrap();
    let mut receipts = vec![];
    for (kind, fixture) in [
        (ApprovalKind::Command, command),
        (ApprovalKind::FileChange, file),
    ] {
        let label = if kind == ApprovalKind::Command {
            "command"
        } else {
            "fileChange"
        };
        let (event, reviewed_host) = if kind == ApprovalKind::Command {
            (
                json!({"threadId":"t","turnId":"u","item":{"id":"i","type":"commandExecution","command":"echo safe","cwd":cwd}}),
                json!({"id":"i","threadId":"t","turnId":"u","kind":"command","command":"echo safe","commandDescriptor":"echo safe","cwd":cwd,"output":"","state":"running","previewComplete":true,"truncated":false}),
            )
        } else {
            (
                json!({"threadId":"t","turnId":"u","item":{"id":"i","type":"fileChange","changes":[{"path":"safe.txt","diff":"+safe change"}]}}),
                json!({"id":"i","threadId":"t","turnId":"u","kind":"fileChange","cwd":"","output":"","state":"running","previewComplete":true,"truncated":false,"paths":["safe.txt"],"diff":"safe.txt\n+safe change"}),
            )
        };
        for case in cases.as_array().unwrap() {
            let mut request = fixture.clone();
            if kind == ApprovalKind::Command {
                request["params"]["cwd"] = json!(cwd);
            }
            if case["present"] == false {
                request["params"]
                    .as_object_mut()
                    .unwrap()
                    .remove("environmentId");
            } else {
                request["params"]["environmentId"] = case["value"].clone();
            }
            let (_, approval) = pending(&request);
            let original = approval.clone();
            let mut native = native_tools::ToolLedger::default();
            let reviewed = native
                .capture(NotificationKind::ItemStarted, &event, &project)
                .unwrap();
            assert!(native.offer(approval.clone(), &project));
            let native_eligible = native.consume_reviewed(&approval, &project, &reviewed);
            if !native_eligible {
                assert!(native.take(&approval)); // native safe decline can still consume it
            }
            let mut host = host_binding::EvidenceLedger::default();
            host.observe(NotificationKind::ItemStarted, &event);
            let host_eligible = host.matches(&approval, &reviewed_host, &project);
            assert_eq!(approval, original);
            let (mut decline_broker, decline_approval) = pending(&request);
            decline_broker
                .decline_approval(&decline_approval.id)
                .unwrap();
            let decline: Value =
                serde_json::from_slice(&decline_broker.take_outbound().unwrap()).unwrap();
            receipts.push(json!({"kind":label,"name":case["name"],"descriptorPreserved":true,
                "native":dispatch(&request,native_eligible,false),"host":dispatch(&request,host_eligible,false),
                "broker":dispatch(&request,true,false),"brokerDirect":dispatch(&request,true,true),"decline":decline}));
        }
    }
    println!("{}", serde_json::to_string(&receipts).unwrap());
}
