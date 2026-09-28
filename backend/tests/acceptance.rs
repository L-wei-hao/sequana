use sequana::engine::{validate_workflow, WorkflowDefinition};
use serde_json::json;

#[test]
fn test_acceptance_workflow_graph_validates() {
    // The V1 reference acceptance workflow from Section 24:
    // Webhook -> Set -> OpenAI -> If -> (true: PostgreSQL -> Respond 200, false: Respond 400)
    let workflow_json = json!({
        "nodes": [
            { "id": "webhook", "node_type": "webhook", "config": { "path": "/candidate" } },
            { "id": "set", "node_type": "set", "config": { "values": { "candidate": { "$from": "/body/candidate" } } } },
            {
                "id": "openai",
                "node_type": "open_ai",
                "config": {
                    "credential_id": "00000000-0000-0000-0000-000000000001",
                    "model": "gpt-5.6-luna",
                    "input": { "$from": "/candidate" },
                    "output": {
                        "type": "json_schema",
                        "name": "candidate_eval",
                        "schema": {
                            "type": "object",
                            "properties": { "qualified": { "type": "boolean" } },
                            "required": ["qualified"]
                        }
                    }
                }
            },
            {
                "id": "if",
                "node_type": "if",
                "config": {
                    "left": { "$from": "/output/qualified" },
                    "operator": "eq",
                    "right": true
                }
            },
            {
                "id": "db",
                "node_type": "postgres",
                "config": {
                    "query": "INSERT INTO candidates (name) VALUES ($1)",
                    "mode": "execute",
                    "params": [
                        { "type": "text", "value": { "$from": "/candidate" } }
                    ]
                }
            },
            {
                "id": "respond_ok",
                "node_type": "respond_to_webhook",
                "config": { "status": 200, "body": { "status": "accepted" } }
            },
            {
                "id": "respond_bad",
                "node_type": "respond_to_webhook",
                "config": { "status": 400, "body": { "status": "rejected" } }
            }
        ],
        "edges": [
            { "source": "webhook", "target": "set" },
            { "source": "set", "target": "openai" },
            { "source": "openai", "target": "if" },
            { "source": "if", "target": "db", "route": "true" },
            { "source": "if", "target": "respond_bad", "route": "false" },
            { "source": "db", "target": "respond_ok" }
        ]
    });

    let definition: WorkflowDefinition =
        serde_json::from_value(workflow_json).expect("valid workflow definition JSON");

    assert!(validate_workflow(&definition).is_ok());
}
