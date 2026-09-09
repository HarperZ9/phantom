# Phantom Boundary Gateway

`phantom boundary` is an opt-in local gateway for OpenAI-compatible clients pointed at `http://127.0.0.1:<port>`. The first release supports only non-streaming `POST /v1/chat/completions` to one exact numeric loopback upstream route from a sealed operator policy. One running gateway serves one sealed policy session; use a separate sealed policy and local bearer token for a separate session.

This controls the first hop for clients configured to use the gateway. It does not control browsers, shell networking, unmanaged SDK clients, provider retention, legal ownership, an already privileged host process, or side channels inside the chosen upstream such as shared mutable state or tenant-isolation defects. The HMAC policy key and gateway auth token are trusted local operator state; same-user host code that can read environment variables or policy files is outside this boundary.

Exact-byte approval is deliberate. It is useful for controlled disclosure tests and fixed request bodies, but ordinary SDK chat requests change every turn. The next workflow to add is staged request review or an explicit trusted-local session policy. There is no implicit allow-all fallback.

Receipts are content-free JSONL records. A `pre_dispatch` receipt records the gateway's local policy decision before an upstream request is sent. A `post_dispatch` receipt records a transport result after dispatch has started, such as a redirect response or oversized upstream response; that stage is not proof that disclosure was prevented, because the local upstream may already have received the approved body.

## Synthetic Local Run

Use a long synthetic key and local bearer token. Do not use provider keys or unpublished material in this workflow.

```powershell
$env:PHANTOM_BOUNDARY_POLICY_KEY = "demo-operator-boundary-key-32-bytes"
$env:PHANTOM_BOUNDARY_AUTH = "demo-local-gateway-bearer-token"

@'
{"model":"local","messages":[{"role":"user","content":"approved synthetic request"}]}
'@ | Set-Content -Encoding UTF8 .\request.json

$contentId = phantom boundary content-id --session demo-session --content .\request.json

$policy = @{
  schema = 1
  policy_version = "demo-policy"
  session_id = "demo-session"
  default_action = "deny"
  remote_fallback = $false
  redirects = @{ follow = $false }
  routes = @(@{
    id = "local-chat"
    method = "POST"
    operation = "chat.completions.create"
    upstream_url = "http://127.0.0.1:45555/v1/chat/completions"
  })
  exceptions = @(@{
    id = "approved-demo-body"
    route_id = "local-chat"
    operation = "chat.completions.create"
    content_id = $contentId.Trim()
    expires_unix_secs = 2000000000
    reason = "synthetic demo"
  })
  limits = @{
    max_request_bytes = 16384
    max_response_bytes = 16384
    max_header_bytes = 8192
    request_timeout_secs = 2
    max_in_flight_requests = 8
  }
  credential_profile = "local-demo-no-upstream-auth-forwarding"
  policy_mac = ""
} | ConvertTo-Json -Depth 8

$policy | Set-Content -Encoding UTF8 .\policy.unsigned.json
phantom boundary seal --policy .\policy.unsigned.json --out .\policy.sealed.json
phantom --json boundary check --policy .\policy.sealed.json --session demo-session --content .\request.json
```

Start a local recorder in one terminal:

```powershell
@'
from http.server import BaseHTTPRequestHandler, HTTPServer
class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get("content-length", "0"))
        body = self.rfile.read(n)
        print(self.path, body.decode("utf-8", "replace"))
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.end_headers()
        self.wfile.write(b'{"id":"synthetic-local","choices":[]}')
HTTPServer(("127.0.0.1", 45555), Handler).serve_forever()
'@ | Set-Content -Encoding UTF8 .\boundary-recorder.py
python .\boundary-recorder.py
```

Start the gateway in another terminal:

```powershell
phantom boundary serve --policy .\policy.sealed.json --listen 127.0.0.1:43117
```

Point a test client at `http://127.0.0.1:43117` with `Authorization: Bearer demo-local-gateway-bearer-token`. The gateway forwards only the approved exact body to the configured loopback upstream and writes content-free receipts to `<data_dir>\logs\boundary-receipts.jsonl` unless `--receipt-log` is supplied.
