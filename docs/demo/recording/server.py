"""Serve local demo files with proxy subscription usage metadata."""

from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from random import randint
from time import sleep


FIXTURES = Path(__file__).resolve().parent / "fixtures"
SUBSCRIPTION_INFO = (
    "upload=1073741824; download=21474836480; "
    "total=107374182400; expire=1893456000"
)


class DemoHandler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(FIXTURES), **kwargs)

    def send_health(self):
        sleep(randint(300, 600) / 1000)
        self.send_response(204)
        self.end_headers()

    def do_HEAD(self):
        if self.path.partition("?")[0] == "/health":
            self.send_health()
            return
        super().do_HEAD()

    def do_GET(self):
        path = self.path.partition("?")[0]
        if path == "/health":
            self.send_health()
            return
        if path.startswith("/traffic/"):
            self.send_response(200)
            self.send_header("Content-Type", "text/plain; charset=utf-8")
            self.end_headers()
            try:
                for _ in range(60):
                    self.wfile.write(b"local demo traffic\n")
                    self.wfile.flush()
                    sleep(1)
            except (BrokenPipeError, ConnectionResetError):
                pass
            return
        super().do_GET()

    def end_headers(self):
        if self.path.partition("?")[0] == "/proxy-provider.yaml":
            self.send_header("subscription-userinfo", SUBSCRIPTION_INFO)
        super().end_headers()


if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", 18080), DemoHandler).serve_forever()
