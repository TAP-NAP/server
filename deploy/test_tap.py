import copy
import hashlib
import importlib.machinery
import importlib.util
import io
import os
from pathlib import Path
import stat
import tempfile
import unittest
from unittest import mock
import urllib.error
import warnings
import zipfile


LOADER = importlib.machinery.SourceFileLoader("tap_under_test", str(Path(__file__).with_name("tap")))
SPEC = importlib.util.spec_from_loader(LOADER.name, LOADER)
tap = importlib.util.module_from_spec(SPEC)
LOADER.exec_module(tap)


def run_record(sha="a" * 40, attempt=1, run_id=101):
    return {"status": "completed", "conclusion": "success", "head_branch": "main",
            "event": "push", "path": tap.WORKFLOW, "head_sha": sha,
            "id": run_id, "run_attempt": attempt,
            "repository": {"full_name": tap.REPO, "id": 5},
            "head_repository": {"full_name": tap.REPO, "id": 5}}


def jobs_record(run):
    return {"jobs": [{"name": "build", "status": "completed", "conclusion": "success",
                      "run_id": run["id"], "run_attempt": run["run_attempt"], "head_sha": run["head_sha"],
                      "steps": [{"name": name, "status": "completed", "conclusion": "success"}
                                for name in ("Test Rust and TypeScript", "Build static site",
                                             "Upload site for manual server deployment")]}]}


def site_zip(extra=(), omit=()):
    entries = [("index.html", b"home"), ("verify/index.html", b"verify"),
               ("privacy/index.html", b"privacy"),
               ("assets/tapcam_verifier_wasm-12345678.wasm", b"wasm")]
    buffer = io.BytesIO()
    with warnings.catch_warnings(), zipfile.ZipFile(buffer, "w") as archive:
        warnings.simplefilter("ignore", UserWarning)
        for name, value in entries + list(extra):
            if name not in omit:
                archive.writestr(name, value)
    return buffer.getvalue()


def build_record(data, sha="a" * 40, artifact_id=10):
    return {"commit": sha, "run_id": 101, "attempt": 1, "artifact_id": artifact_id,
            "digest": "sha256:" + hashlib.sha256(data).hexdigest()}


def artifact_record(data, run):
    return {"name": "tapnap-web-" + run["head_sha"] + "-" + str(run["run_attempt"]),
            "expired": False, "id": 10, "size_in_bytes": len(data),
            "digest": "sha256:" + hashlib.sha256(data).hexdigest(),
            "workflow_run": {"id": run["id"], "head_sha": run["head_sha"],
                             "head_branch": "main", "repository_id": 5, "head_repository_id": 5}}


class DeployTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.root = self.base / "web"
        self.quiet = mock.patch.object(tap, "say")
        self.quiet.start()
        self.addCleanup(self.quiet.stop)

    def publish_fixture(self, data=None, sha="a" * 40, artifact_id=10):
        data = data or site_zip()
        stage = self.base / ("stage-" + str(artifact_id))
        archive = self.base / (str(artifact_id) + ".zip")
        archive.write_bytes(data)
        tap.unpack(archive, stage)
        build = build_record(data, sha, artifact_id)
        with tap.locked(self.root):
            tap.publish(self.root, stage, build)
        return os.readlink(self.root / "current")

    def mock_update(self, data, *, dry=False, run=None, failure=None, jobs=None):
        run = run or run_record("b" * 40)
        def fetch(build, target, token):
            if failure:
                raise failure
            target.write_bytes(data)
        with mock.patch.object(tap, "select_build", return_value=build_record(data, "b" * 40, 11)), \
                mock.patch.object(tap, "download", side_effect=fetch), \
                mock.patch.object(tap, "api", side_effect=[run, jobs if jobs is not None else jobs_record(run)]):
            tap.update(self.root, 101, "secret", dry)

    def test_trusted_selection_and_latest_selection(self):
        run = run_record()
        artifact = artifact_record(site_zip(), run)
        for run_id, first in [(101, run), (None, {"workflow_runs": [run]})]:
            with self.subTest(run_id=run_id), mock.patch.object(tap, "api", side_effect=[first, jobs_record(run), {"artifacts": [artifact]}]):
                selected = tap.select_build(run_id, "secret")
                self.assertEqual((selected["commit"], selected["attempt"], selected["artifact_id"]), ("a" * 40, 1, 10))

    def test_untrusted_runs_are_rejected(self):
        changes = [("status", "queued"), ("head_branch", "feature"),
                   ("event", "pull_request"), ("path", ".github/workflows/other.yml"),
                   ("repository", {"full_name": "other/repo", "id": 5}),
                   ("head_repository", {"full_name": "fork/TAPCamVerifier", "id": 6}),
                   ("head_sha", "short"), ("run_attempt", 0), ("run_attempt", "1"), ("id", -1)]
        changes += [("conclusion", value) for value in ("cancelled", "skipped", "action_required", "stale")]
        for field, value in changes:
            run = run_record()
            run[field] = value
            with self.subTest(field=field, value=value), self.assertRaises(tap.Failure):
                tap.check_run(run)

    def test_successful_build_can_publish_while_pages_pending_or_failed(self):
        for status, conclusion in [("in_progress", None), ("completed", "failure"), ("completed", "timed_out")]:
            run = run_record(attempt=2)
            run.update(status=status, conclusion=conclusion)
            artifact = artifact_record(site_zip(), run)
            with self.subTest(status=status, conclusion=conclusion), mock.patch.object(
                    tap, "api", side_effect=[run, jobs_record(run), {"artifacts": [artifact]}]) as api:
                self.assertEqual(tap.select_build(101, "secret")["attempt"], 2)
                self.assertEqual(api.call_args_list[1], mock.call(
                    "/actions/runs/101/attempts/2/jobs?per_page=100", "secret"))

    def test_failed_skipped_missing_or_wrong_origin_build_is_rejected(self):
        run = run_record()
        good = jobs_record(run)
        variants = [{"jobs": []}, {"jobs": good["jobs"] * 2}]
        for field, value in [("name", "other"), ("status", "in_progress"), ("conclusion", "failure"),
                             ("conclusion", "skipped"), ("run_id", 102), ("run_attempt", 2),
                             ("head_sha", "b" * 40), ("steps", [])]:
            jobs = copy.deepcopy(good)
            jobs["jobs"][0][field] = value
            variants.append(jobs)
        for index in range(3):
            for field, value in [("status", "in_progress"), ("conclusion", "failure"), ("conclusion", "skipped")]:
                jobs = copy.deepcopy(good)
                jobs["jobs"][0]["steps"][index][field] = value
                variants.append(jobs)
            jobs = copy.deepcopy(good)
            jobs["jobs"][0]["steps"].append(copy.deepcopy(jobs["jobs"][0]["steps"][index]))
            variants.append(jobs)
        for jobs in variants:
            with self.subTest(jobs=jobs), mock.patch.object(tap, "api", return_value=jobs), self.assertRaises(tap.Failure):
                tap.check_build(run, "secret")

    def test_latest_selection_falls_back_after_bad_build_but_not_network_failure(self):
        recent = run_record("b" * 40, run_id=102)
        older = run_record()
        bad = jobs_record(recent)
        bad["jobs"][0]["conclusion"] = "failure"
        listing = {"workflow_runs": [recent, older]}
        with mock.patch.object(tap, "api", side_effect=[listing, bad, jobs_record(older),
                                                       {"artifacts": [artifact_record(site_zip(), older)]}]) as api:
            self.assertEqual(tap.select_build(None, "secret")["run_id"], 101)
            self.assertNotIn("status=", api.call_args_list[0].args[0])
            self.assertIn("per_page=20", api.call_args_list[0].args[0])
        with mock.patch.object(tap, "api", side_effect=[listing, urllib.error.URLError("offline")]) as api:
            with self.assertRaises(urllib.error.URLError):
                tap.select_build(None, "secret")
            self.assertEqual(api.call_count, 2)

    def test_bad_artifact_identity_or_digest_is_rejected(self):
        run = run_record()
        good = artifact_record(site_zip(), run)
        variants = []
        for field, value in [("expired", True), ("digest", None), ("digest", "sha256:bad"),
                             ("workflow_run", {}), ("size_in_bytes", tap.MAX_ZIP + 1),
                             ("name", "tapnap-web-" + run["head_sha"] + "-2")]:
            item = copy.deepcopy(good)
            item[field] = value
            variants.append(item)
        for field, value in [("id", 999), ("head_sha", "b" * 40), ("head_repository_id", 6),
                             ("repository_id", 6), ("head_branch", "feature")]:
            item = copy.deepcopy(good)
            item["workflow_run"][field] = value
            variants.append(item)
        for item in variants:
            with self.subTest(artifact=item), mock.patch.object(tap, "api", side_effect=[run, jobs_record(run), {"artifacts": [item]}]):
                with self.assertRaises(tap.Failure):
                    tap.select_build(101, "secret")
        with mock.patch.object(tap, "api", side_effect=[run, jobs_record(run), {"artifacts": [good, good]}]):
            with self.assertRaises(tap.Failure):
                tap.select_build(101, "secret")

    def test_unsafe_zips_leave_existing_current_unchanged(self):
        old = self.publish_fixture()
        link = zipfile.ZipInfo("assets/symlink")
        link.create_system = 3
        link.external_attr = (stat.S_IFLNK | 0o777) << 16
        encrypted = bytearray(site_zip())
        for signature, flag_offset in [(b"PK\x03\x04", 6), (b"PK\x01\x02", 8)]:
            offset = encrypted.find(signature)
            encrypted[offset + flag_offset] |= 1
        cases = [bytes(encrypted), site_zip(extra=[("../../escaped", b"bad")]),
                 site_zip(extra=[("/absolute", b"bad")]),
                 site_zip(extra=[("assets/../escaped", b"bad")]),
                 site_zip(extra=[("assets\\escaped", b"bad")]),
                 site_zip(extra=[(".env", b"bad")]),
                 site_zip(extra=[(link, b"../../outside")]),
                 site_zip(extra=[("index.html", b"duplicate")]),
                 site_zip(omit=["privacy/index.html"]),
                 site_zip(omit=["assets/tapcam_verifier_wasm-12345678.wasm"])]
        for data in cases:
            with self.subTest(size=len(data)), self.assertRaises((tap.Failure, OSError)):
                self.mock_update(data)
            self.assertEqual(os.readlink(self.root / "current"), old)
            self.assertEqual((self.root / "current/index.html").read_bytes(), b"home")
        self.assertFalse((self.base / "escaped").exists())

    def test_zip_size_and_count_limits_preserve_current(self):
        old = self.publish_fixture()
        for limit, value in [("MAX_EXPANDED", 1), ("MAX_FILES", 1)]:
            with self.subTest(limit=limit), mock.patch.object(tap, limit, value), self.assertRaises(tap.Failure):
                self.mock_update(site_zip())
            self.assertEqual(os.readlink(self.root / "current"), old)

    def test_download_digest_size_and_redirect_credential_boundary(self):
        data = site_zip()
        storage = "https://storage.example/artifact?signature=short-lived"
        next_storage = "https://second.example/file"
        errors = [urllib.error.HTTPError(tap.API, 302, "redirect", {"Location": storage}, io.BytesIO()),
                  urllib.error.HTTPError(storage, 307, "redirect", {"Location": next_storage}, io.BytesIO())]
        with mock.patch.object(tap, "request", side_effect=errors + [io.BytesIO(data)]) as request:
            tap.download(build_record(data), self.base / "download.zip", "secret")
            self.assertEqual([call.args[1] for call in request.call_args_list], ["secret", "", ""])
            self.assertEqual(request.call_args.args[0], next_storage)
        with mock.patch.object(tap, "request", return_value=io.BytesIO(b"corrupted")), self.assertRaises(tap.Failure):
            tap.download(build_record(data), self.base / "wrong.zip", "secret")
        with mock.patch.object(tap, "MAX_ZIP", 1), mock.patch.object(tap, "request", return_value=io.BytesIO(data)), self.assertRaises(tap.Failure):
            tap.download(build_record(data), self.base / "oversized.zip", "secret")

    def test_request_never_sends_token_to_other_hosts(self):
        for url in ["https://storage.example/file", "https://api.github.com.evil/file", "http://api.github.com/file"]:
            with self.subTest(url=url), self.assertRaises(tap.Failure):
                tap.request(url, "secret")
        opener = mock.Mock()
        with mock.patch.object(tap.urllib.request, "build_opener", return_value=opener):
            tap.request(tap.API + "/test", "secret")
            self.assertEqual(opener.open.call_args.args[0].get_header("Authorization"), "Bearer secret")
            tap.request("https://storage.example/file")
            self.assertIsNone(opener.open.call_args.args[0].get_header("Authorization"))

    def test_two_publishes_and_rollback_retain_hashed_assets(self):
        first = self.publish_fixture(site_zip(extra=[("assets/main-11111111.js", b"old")]))
        second = self.publish_fixture(site_zip(extra=[("assets/main-22222222.js", b"new")]), "b" * 40, 11)
        self.assertNotEqual(first, second)
        self.assertEqual(os.readlink(self.root / "previous"), first)
        self.assertEqual((self.root / "shared/assets/main-11111111.js").read_bytes(), b"old")
        self.assertEqual((self.root / "shared/assets/main-22222222.js").read_bytes(), b"new")
        tap.rollback(self.root)
        self.assertEqual(os.readlink(self.root / "current"), first)
        self.assertEqual(os.readlink(self.root / "previous"), first)
        tap.rollback(self.root)
        self.assertEqual(os.readlink(self.root / "current"), first)

    def test_asset_collision_leaves_current_and_existing_bytes_intact(self):
        old = self.publish_fixture(site_zip(extra=[("assets/main-11111111.js", b"old")]))
        with self.assertRaises(tap.Failure):
            self.publish_fixture(site_zip(extra=[("assets/main-11111111.js", b"different")]), "b" * 40, 11)
        self.assertEqual(os.readlink(self.root / "current"), old)
        self.assertEqual((self.root / "shared/assets/main-11111111.js").read_bytes(), b"old")
        self.assertEqual(len(list((self.root / "releases").iterdir())), 1)

    def test_failed_network_or_rerun_keeps_current(self):
        old = self.publish_fixture()
        with self.assertRaises(urllib.error.URLError):
            self.mock_update(site_zip(), failure=urllib.error.URLError("offline"))
        for run in [run_record("b" * 40, attempt=2), run_record("c" * 40)]:
            with self.subTest(run=run), self.assertRaises(tap.Failure):
                self.mock_update(site_zip(), run=run)
        with self.assertRaises(tap.Failure):
            self.mock_update(site_zip(), jobs={"jobs": []})
        self.assertEqual(os.readlink(self.root / "current"), old)
        self.assertEqual(len(list((self.root / "releases").iterdir())), 1)
        self.assertFalse(list(self.root.glob(".tap-download-*")))

    def test_dry_run_never_creates_root_or_changes_existing_site(self):
        self.mock_update(site_zip(), dry=True)
        self.assertFalse(self.root.exists())
        old = self.publish_fixture()
        before = sorted(str(path.relative_to(self.root)) for path in self.root.rglob("*"))
        self.mock_update(site_zip(), dry=True)
        self.assertEqual(os.readlink(self.root / "current"), old)
        self.assertEqual(sorted(str(path.relative_to(self.root)) for path in self.root.rglob("*")), before)

    def test_lock_contention_prevents_other_mutation(self):
        self.publish_fixture()
        with tap.locked(self.root):
            with self.assertRaises(tap.Failure):
                with tap.locked(self.root):
                    self.fail("Second deployment acquired the lock")
            with self.assertRaises(tap.Failure):
                tap.rollback(self.root)

    def test_failed_current_commit_keeps_previous_rollback_available(self):
        first = self.publish_fixture(artifact_id=10)
        second = self.publish_fixture(sha="b" * 40, artifact_id=11)
        replace = tap.replace_link

        def fail_current(root, name, target):
            if name == "current":
                raise OSError("simulated current commit failure")
            return replace(root, name, target)

        for operation in ("update", "rollback"):
            with self.subTest(operation=operation):
                replace(self.root, "previous", first)
                with mock.patch.object(tap, "replace_link", side_effect=fail_current), self.assertRaises(OSError):
                    if operation == "update":
                        self.publish_fixture(sha="c" * 40, artifact_id=12)
                    else:
                        tap.rollback(self.root)
                self.assertEqual(os.readlink(self.root / "current"), second)
                self.assertEqual(os.readlink(self.root / "previous"), first)

    def test_published_directories_remain_readable_with_restrictive_umask(self):
        original = os.umask(0o077)
        try:
            self.publish_fixture()
        finally:
            os.umask(original)
        for path in [self.root] + [p for p in self.root.rglob("*") if p.is_dir()]:
            with self.subTest(path=path):
                self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o755)

    def test_managed_publish_directories_cannot_be_symlinks(self):
        for index, name in enumerate(("releases", "shared", "shared/assets")):
            self.root = self.base / ("root-" + str(index))
            outside = self.base / ("outside-" + str(index))
            outside.mkdir()
            target = self.root / name
            target.parent.mkdir(parents=True)
            target.symlink_to(outside, target_is_directory=True)
            with self.subTest(directory=name), self.assertRaises(tap.Failure):
                self.publish_fixture(artifact_id=30 + index)
            self.assertFalse((self.root / "current").exists())
            self.assertEqual(list(outside.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
