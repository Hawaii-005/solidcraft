"""A stub OpenAI-compatible endpoint for testing SolidCraft's API agent on the tablet.

First request: answers with a tool call (execute sketch.create on the XY plane).
After the tool result comes back: answers with plain text.

Run: python3 solidcraft-stub-llm.py [port]
"""
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8787


class Handler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):  # noqa: A002
        sys.stderr.write("[stub] " + format % args + "\n")

    def _json(self, obj, status=200):
        body = json.dumps(obj).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path.startswith("/v1/models"):
            self._json({"object": "list", "data": [{"id": "stub-model", "object": "model"}]})
        else:
            self._json({"error": "not found"}, 404)

    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        req = json.loads(self.rfile.read(n) or b"{}")
        msgs = req.get("messages", [])
        tools_seen = [t.get("function", {}).get("name") for t in req.get("tools", [])]
        auth = self.headers.get("Authorization")
        print(f"[stub] request: {len(msgs)} messages, tools={tools_seen}, auth={auth}", flush=True)
        if any(m.get("role") == "tool" for m in msgs):
            self._json({
                "choices": [{"index": 0, "message": {"role": "assistant", "content": "Feito: criei o sketch no plano XY."},
                             "finish_reason": "stop"}]
            })
            return
        self._json({
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": None,
                    "tool_calls": [{
                        "id": "call-1",
                        "type": "function",
                        "function": {"name": "execute", "arguments": json.dumps({"command": "sketch.create", "params": {"plane": "XY"}})},
                    }],
                },
                "finish_reason": "tool_calls",
            }]
        })


if __name__ == "__main__":
    print(f"[stub] listening on 0.0.0.0:{PORT}", flush=True)
    ThreadingHTTPServer(("0.0.0.0", PORT), Handler).serve_forever()
