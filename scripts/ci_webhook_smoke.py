import json
import os
import urllib.request

BASE = "http://localhost:8080"
TOKEN = os.environ["SEQUANA_ADMIN_TOKEN"]
TENANT = "00000000-0000-0000-0000-000000000001"


def request(path, *, data=None, admin=False, headers=None):
    body = None if data is None else json.dumps(data).encode()
    request_headers = {"Content-Type": "application/json"}
    if admin:
        request_headers.update(
            {
                "Authorization": f"Bearer {TOKEN}",
                "x-tenant-id": TENANT,
            }
        )
    if headers:
        request_headers.update(headers)

    req = urllib.request.Request(
        BASE + path,
        data=body,
        headers=request_headers,
        method="POST" if data is not None else "GET",
    )
    with urllib.request.urlopen(req) as response:
        payload = response.read()
        return json.loads(payload) if payload else None


created = request(
    "/api/workflows",
    admin=True,
    data={
        "name": "CI Webhook",
        "definition": {
            "nodes": [
                {"id": "webhook", "node_type": "webhook", "config": {}},
                {
                    "id": "set",
                    "node_type": "set",
                    "config": {"values": {"valid": True}, "merge_input": False},
                },
                {
                    "id": "if",
                    "node_type": "if",
                    "config": {
                        "left": {"$from": "/valid"},
                        "operator": "eq",
                        "right": True,
                    },
                },
                {
                    "id": "db",
                    "node_type": "postgres",
                    "config": {"query": "SELECT true AS ok", "mode": "query"},
                },
                {
                    "id": "ok",
                    "node_type": "respond_to_webhook",
                    "config": {"status": 200, "body": {"$from": "/0"}},
                },
                {
                    "id": "bad",
                    "node_type": "respond_to_webhook",
                    "config": {"status": 400, "body": {"error": "invalid"}},
                },
            ],
            "edges": [
                {"source": "webhook", "target": "set"},
                {"source": "set", "target": "if"},
                {"source": "if", "target": "db", "route": "true"},
                {"source": "if", "target": "bad", "route": "false"},
                {"source": "db", "target": "ok"},
            ],
        },
    },
)

workflow_id = created["workflow_id"]
version_id = created["version_id"]

request(
    f"/api/workflows/{workflow_id}/activate/{version_id}",
    admin=True,
    data={},
)

webhook_headers = {"Idempotency-Key": "ci-duplicate"}
first = request(
    f"/webhook/{workflow_id}/webhook",
    data={"candidate": "Ada"},
    headers=webhook_headers,
)
second = request(
    f"/webhook/{workflow_id}/webhook",
    data={"candidate": "Ada"},
    headers=webhook_headers,
)

assert first == {"ok": True}
assert second == {"ok": True}

history = request(
    f"/api/executions?workflow_id={workflow_id}",
    admin=True,
)
assert len([row for row in history if row["trigger_type"] == "webhook"]) == 1
