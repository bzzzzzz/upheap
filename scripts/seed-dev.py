#!/usr/bin/env python3
"""Resets the local development database, starts the API server on it, seeds it, and keeps the server running.

    scripts/seed-dev.py              # fresh data/dev.db, server on :8137 until Ctrl-C
    scripts/seed-dev.py --no-serve   # same, but stop the server once seeded (the data stays)

Then run the web app with `(cd web && npm run dev)` and sign in at http://localhost:5173.

Test-only accounts, for local development; every password is dev-password-123:

    alex   admin of Acme
    blair  member of Acme, admin of Platform
    casey  member of Acme and Platform

Acme has the Product and Operations heaps; Platform has Infrastructure. Each heap mixes open, in-progress, blocked
(most waiting on another item), done and won't-do items, with tags and due dates, added and moved by different people.
"""

from __future__ import annotations

import argparse
import datetime
import http.cookiejar
import json
import os
import signal
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DB = ROOT / "data" / "dev.db"
PORT = 8137
BASE = f"http://127.0.0.1:{PORT}"
PASSWORD = "dev-password-123"

USERS = {
    "alex": "Alex Rivera",
    "blair": "Blair Chen",
    "casey": "Casey Morgan",
}


def in_days(n: int) -> str:
    return (datetime.date.today() + datetime.timedelta(days=n)).isoformat()


class Client:
    """One signed-in user, with their own session cookie."""

    def __init__(self, username: str):
        self.username = username
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))

    def call(self, method: str, path: str, body=None, version=None):
        headers = {"X-CSRF-Protection": "1", "Content-Type": "application/json"}
        if version is not None:
            headers["If-Match"] = f'"{version}"'
        data = None if body is None else json.dumps(body).encode()
        req = urllib.request.Request(BASE + "/api/v1" + path, data=data, method=method, headers=headers)
        try:
            with self.opener.open(req) as res:
                raw = res.read()
        except urllib.error.HTTPError as e:
            sys.exit(f"{self.username}: {method} {path} failed with {e.code}: {e.read().decode()}")
        return json.loads(raw) if raw else None

    def register(self):
        self.call(
            "POST",
            "/auth/register",
            {
                "email": f"{self.username}@example.test",
                "username": self.username,
                "display_name": USERS[self.username],
                "password": PASSWORD,
            },
        )

    def user_id(self, username: str) -> str:
        found = self.call("GET", "/users?query=" + urllib.parse.quote(username))["users"]
        return next(u["id"] for u in found if u["username"] == username)


class Heap:
    """A heap as seen by one user; `as_` switches who acts next, so creators and closers vary."""

    def __init__(self, client: Client, team: str, slug: str):
        self.client, self.path, self.tags, self.items = client, f"/teams/{team}/heaps/{slug}", {}, {}

    def as_(self, client: Client) -> "Heap":
        other = Heap(client, "", "")
        other.path, other.tags, other.items = self.path, self.tags, self.items
        return other

    def tag(self, *names: str):
        for name in names:
            self.tags[name] = self.client.call("POST", self.path + "/tags", {"name": name})["id"]

    def add(self, ref: str, title: str, need: str, effort=None, impact=None, due=None, firm=False, tags=(), desc=None):
        body = {"title": title, "need": need, "effort": effort, "impact": impact, "description": desc}
        if due is not None:
            body["due"] = {"date": in_days(due), "firm": firm}
        body["tag_ids"] = [self.tags[t] for t in tags]
        self.items[ref] = self.client.call("POST", self.path + "/items", body)

    def patch(self, ref: str, body: dict):
        item = self.items[ref]
        self.items[ref] = self.client.call("PATCH", f"{self.path}/items/{item['key']}", body, item["version"])

    def start(self, ref: str):
        self.patch(ref, {"state": "in_progress"})

    def block(self, ref: str, by: str | None = None):
        body = {"state": "blocked"}
        if by is not None:
            body["blocked_by_item_id"] = self.items[by]["id"]
        self.patch(ref, body)

    def done(self, ref: str):
        self.patch(ref, {"state": "done"})

    def wont_do(self, ref: str):
        self.patch(ref, {"state": "wont_do"})


def seed():
    alex, blair, casey = (Client(u) for u in USERS)
    for c in (alex, blair, casey):
        c.register()

    # Acme: everyone, run by Alex.
    alex.call("POST", "/teams", {"name": "Acme", "slug": "acme"})
    for username in ("blair", "casey"):
        alex.call("POST", "/teams/acme/members", {"user_id": alex.user_id(username)})
    alex.call("POST", "/teams/acme/heaps", {"name": "Product", "slug": "product"})
    alex.call("POST", "/teams/acme/heaps", {"name": "Operations", "slug": "operations"})

    # Platform: Blair and Casey, run by Blair, who is only a member of Acme.
    blair.call("POST", "/teams", {"name": "Platform", "slug": "platform"})
    blair.call("POST", "/teams/platform/members", {"user_id": blair.user_id("casey")})
    blair.call("POST", "/teams/platform/heaps", {"name": "Infrastructure", "slug": "infrastructure"})

    seed_product(Heap(alex, "acme", "product"), blair, casey)
    seed_operations(Heap(alex, "acme", "operations"), blair, casey)
    seed_infrastructure(Heap(blair, "platform", "infrastructure"), casey)
    seed_personal(alex, blair, casey)


def seed_product(h: Heap, blair: Client, casey: Client):
    b, c = h.as_(blair), h.as_(casey)
    h.tag("frontend", "backend", "design", "customer")
    h.add("search", "Search across all heaps", "expected", "l", "xl", due=21, tags=["frontend", "backend"])
    h.add("api", "Search API endpoint", "expected", "m", "l", due=14, tags=["backend"])
    b.add("mockups", "Mockups for the search results page", "expected", "s", "m", due=7, tags=["design"])
    c.add("dark", "Dark mode for the settings page", "nice_to_have", "s", "m", tags=["frontend", "design"])
    h.add("export", "CSV export for enterprise customers", "critical", "m", "xl", due=5, firm=True, tags=["customer"])
    b.add("sso", "SSO with Okta", "expected", "xl", "xl", due=45, tags=["backend", "customer"])
    b.add("saml", "SAML metadata from the customer's IT team", "expected", "s", "l", tags=["customer"])
    c.add("onboard", "Rework the onboarding checklist", "nice_to_have", "m", "l", tags=["frontend"])
    c.add("emoji", "Emoji reactions on items", "idea", "m", "s", tags=["frontend"])
    h.add("perf", "List page takes 2s on large heaps", "critical", "m", "l", due=-2, firm=True, tags=["frontend"])
    b.add("i18n", "Translate the app into German", "idea", "xl", "m")
    c.add("a11y", "Fix keyboard focus in the item dialog", "expected", "s", "m", due=10, tags=["frontend"])
    h.add("pricing", "Pricing page refresh", "nice_to_have", "m", "m", tags=["design", "customer"])
    b.add("webhooks", "Outgoing webhooks", "nice_to_have", "l", "l", tags=["backend"])
    c.add("typo", "Typo on the sign-in page", "expected", "s", "s", tags=["frontend"])
    h.add("legacy", "Keep the v1 export format", "nice_to_have", "m", "s", tags=["backend"])
    b.add("tour", "Animated product tour", "idea", "l", "s", tags=["design"])

    # Search waits on its API, which waits on the mockups; SSO waits on the customer's metadata.
    b.start("mockups")
    h.start("api")
    h.block("search", by="api")
    b.block("sso", by="saml")
    c.start("a11y")
    c.start("perf")
    h.block("export", by="perf")
    c.block("onboard")  # waiting on copy from marketing, which isn't an item here
    c.done("typo")
    c.done("dark")
    b.done("webhooks")
    h.wont_do("legacy")
    b.wont_do("tour")


def seed_operations(h: Heap, blair: Client, casey: Client):
    b, c = h.as_(blair), h.as_(casey)
    h.tag("security", "billing", "hiring")
    h.add("tls", "Renew the TLS certificate", "critical", "s", "xl", due=2, firm=True, tags=["security"])
    b.add("audit", "SOC 2 evidence for Q4", "critical", "l", "xl", due=30, firm=True, tags=["security"])
    b.add("policy", "Write the access review policy", "expected", "m", "l", due=12, tags=["security"])
    c.add("vendor", "Pick a new payroll vendor", "expected", "m", "m", due=18, tags=["billing"])
    c.add("contract", "Legal review of the payroll contract", "expected", "s", "m", tags=["billing"])
    h.add("invoice", "Chase the overdue invoice from Globex", "expected", "s", "l", due=-5, tags=["billing"])
    h.add("hire", "Hire a support engineer", "expected", "xl", "l", due=60, tags=["hiring"])
    b.add("jd", "Job description for the support role", "expected", "s", "m", tags=["hiring"])
    c.add("laptops", "Order laptops for new starters", "nice_to_have", "s", "s", tags=["hiring"])
    h.add("offsite", "Plan the spring offsite", "nice_to_have", "m", "m", due=40)
    b.add("fax", "Set up a fax line", "idea", "s", "s")
    c.add("backup", "Test restoring last week's backup", "critical", "m", "xl", due=-1, firm=True, tags=["security"])

    b.start("policy")
    b.block("audit", by="policy")
    c.block("vendor", by="contract")
    h.block("hire", by="jd")
    b.start("jd")
    h.start("tls")
    c.block("backup")  # waiting for the storage provider to grant restore access
    c.done("laptops")
    h.done("invoice")
    b.wont_do("fax")


def seed_infrastructure(h: Heap, casey: Client):
    c = h.as_(casey)
    h.tag("k8s", "database", "observability", "cost")
    h.add("pg", "Upgrade Postgres to 17", "critical", "l", "xl", due=9, firm=True, tags=["database"])
    c.add("replica", "Add a read replica", "expected", "m", "l", tags=["database"])
    h.add("cluster", "Move staging to the new cluster", "expected", "l", "l", due=25, tags=["k8s"])
    c.add("helm", "Port the Helm charts to Kustomize", "expected", "m", "m", tags=["k8s"])
    c.add("alerts", "Page on error-rate spikes, not single errors", "expected", "s", "l", tags=["observability"])
    h.add("traces", "Distributed tracing for the API", "nice_to_have", "l", "l", tags=["observability"])
    h.add("spot", "Run batch jobs on spot instances", "nice_to_have", "m", "l", tags=["cost", "k8s"])
    c.add("logs", "Cut log retention to 30 days", "expected", "s", "m", due=-3, tags=["cost"])
    h.add("dns", "Move DNS to the new provider", "expected", "m", "m", due=15)
    c.add("ipv6", "IPv6 for the public load balancer", "idea", "m", "s")
    h.add("mesh", "Adopt a service mesh", "idea", "xl", "m", tags=["k8s"])
    c.add("creds", "Rotate the database credentials", "expected", "s", "l", due=4, firm=True, tags=["database"])
    h.add("queue", "Dashboard for queue depth", "nice_to_have", "s", "m", tags=["observability"])
    c.add("buckets", "Delete unused storage buckets", "nice_to_have", "s", "m", tags=["cost"])
    h.add("runbook", "Runbook for a failed deploy", "expected", "m", "l", due=11)
    c.add("arm", "Try ARM nodes for the API", "idea", "m", "m", tags=["cost", "k8s"])

    h.start("pg")
    c.block("replica", by="pg")
    c.start("helm")
    h.block("cluster", by="helm")
    c.start("alerts")
    h.block("traces", by="alerts")
    h.block("dns")  # waiting on the registrar's transfer lock
    c.done("logs")
    h.done("spot")
    c.wont_do("ipv6")
    h.wont_do("mesh")


def seed_personal(alex: Client, blair: Client, casey: Client):
    for client, items in (
        (alex, [("Book the dentist", "expected", 6), ("Read the SRE book", "idea", None), ("Renew passport", "critical", 20)]),
        (blair, [("Prepare the 1:1 agenda", "expected", 1), ("Learn Rust lifetimes properly", "nice_to_have", None)]),
        (casey, [("Expense the conference trip", "expected", -1), ("Try the new standing desk", "idea", None)]),
    ):
        h = Heap(client, client.username, "personal")
        for i, (title, need, due) in enumerate(items):
            h.add(str(i), title, need, due=due)
        h.start("0")
        if len(items) > 2:
            h.done("1")


def port_in_use() -> bool:
    with socket.socket() as s:
        return s.connect_ex(("127.0.0.1", PORT)) == 0


def wait_ready(server: subprocess.Popen):
    for _ in range(600):  # a first `cargo run` can take a few minutes to build
        if server.poll() is not None:
            sys.exit("The server exited before it was ready.")
        try:
            with urllib.request.urlopen(BASE + "/readyz", timeout=1):
                return
        except OSError:
            time.sleep(1)
    sys.exit("The server didn't become ready in time.")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--no-serve", action="store_true", help="stop the server once the data is seeded")
    args = parser.parse_args()

    if port_in_use():
        sys.exit(f"Something is already listening on port {PORT}. Stop the dev server first: this script resets its database.")

    DB.parent.mkdir(exist_ok=True)
    for path in DB.parent.glob(DB.name + "*"):  # the database and its -wal and -shm files
        path.unlink()

    env = os.environ | {
        "DATABASE_URL": f"sqlite://{DB.relative_to(ROOT)}?mode=rwc",
        "BASE_URL": "http://localhost:5173",
        "BIND_ADDR": f"127.0.0.1:{PORT}",
        "LOG_FORMAT": "pretty",
    }
    server = subprocess.Popen(["cargo", "run", "-p", "server"], cwd=ROOT, env=env)
    try:
        wait_ready(server)
        seed()
        print(
            "\nSeeded a fresh data/dev.db. Run `(cd web && npm run dev)` and sign in at http://localhost:5173 as\n"
            f"alex, blair or casey, password {PASSWORD}.\n",
            flush=True,
        )
        if not args.no_serve:
            server.wait()
    except KeyboardInterrupt:
        pass
    finally:
        if server.poll() is None:
            server.send_signal(signal.SIGINT)
            server.wait()


if __name__ == "__main__":
    main()
