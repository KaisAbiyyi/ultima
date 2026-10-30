//! Test-only executable: exercise actual native, host and broker code together.
#[path = "../../../src-tauri/src/approval_binding.rs"]
pub mod host_binding;
#[path = "../../../experiments/native-renderer/src/native_tools.rs"]
pub mod native_tools;

use serde_json::{json, Value};
use ultima_protocol_bridge::{Approval, ApprovalDecision, Broker, Event, Limits, NotificationKind};

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

fn dispatch(request: &Value, eligible: bool) -> Value {
    let (mut broker, approval) = pending(request);
    let accepted = eligible
        && broker
            .decide_exact_approval(&approval, ApprovalDecision::Accept, true)
            .is_ok();
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
    let fixture: Value =
        serde_json::from_str(include_str!("../../fixtures/nc10g-local-approval.json")).unwrap();
    let cases: Value =
        serde_json::from_str(include_str!("../../fixtures/nc10g-environments.json")).unwrap();
    let project = std::env::current_dir().unwrap().canonicalize().unwrap();
    let cwd = project.to_str().unwrap();
    let event = json!({"threadId":"t","turnId":"u","item":{"id":"i",
        "type":"commandExecution","command":"echo safe","cwd":cwd}});
    let reviewed_host = json!({"id":"i","threadId":"t","turnId":"u","kind":"command",
        "command":"echo safe","commandDescriptor":"echo safe","cwd":cwd,"output":"",
        "state":"running","previewComplete":true,"truncated":false});
    let mut receipts = vec![];
    for case in cases.as_array().unwrap() {
        let mut request = fixture.clone();
        request["params"]["cwd"] = json!(cwd);
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
        let mut host = host_binding::EvidenceLedger::default();
        host.observe(NotificationKind::ItemStarted, &event);
        let host_eligible = host.matches(&approval, &reviewed_host, &project);
        assert_eq!(approval, original);
        // Safe decline must work even for the original unsupported descriptor.
        let (mut decline_broker, decline_approval) = pending(&request);
        decline_broker
            .decline_approval(&decline_approval.id)
            .unwrap();
        let decline: Value =
            serde_json::from_slice(&decline_broker.take_outbound().unwrap()).unwrap();
        receipts.push(json!({"name":case["name"],"descriptorPreserved":true,
            "native":dispatch(&request,native_eligible),"host":dispatch(&request,host_eligible),
            "broker":dispatch(&request,true),"decline":decline}));
    }
    println!("{}", serde_json::to_string(&receipts).unwrap());
}
