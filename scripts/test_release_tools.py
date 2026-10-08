"""Release contracts: inconsistent metadata or incomplete assets must fail closed."""

import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import subprocess

import release_notes
import update_manifest
import publish_release


class ReleaseToolsTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.version = "0.2.0"
        self.tag = "v0.2.0"
        self.detail = self.root / "docs/06-delivery/versions/v0.2.0.md"
        self.detail.parent.mkdir(parents=True)
        (self.root / "Cargo.toml").write_text('[workspace.package]\nversion = "0.2.0"\n')
        (self.root / "Cargo.lock").write_text(
            '\n'.join(f'[[package]]\nname = "{name}"\nversion = "0.2.0"\n'
                      for name in release_notes.WORKSPACE_PACKAGES)
        )
        self.detail.write_text(
            '# Tiny MD v0.2.0\n\n> 发布日期：2026-10-08\n> 状态：Review\n\n'
            '## 版本概述\n\n支持在线更新。\n\n## 改进\n\n- 下载校验。\n\n'
            '## 验证结果\n\n仅供工程审计。\n\n## 变更依据\n\n比较链接。\n\n'
            '[返回版本总览](index.md)\n', encoding='utf-8'
        )
        (self.detail.parent / 'index.md').write_text(
            '当前代码版本为 `0.2.0`。\n\n| [v0.2.0](v0.2.0.md) | 2026-10-08 | 待发布 | 在线更新 |\n', encoding='utf-8'
        )
        (self.root / 'CHANGELOG.md').write_text(
            '## [0.2.0] - 2026-10-08\n\n- 在线更新。\n\n'
            '[0.2.0]: docs/06-delivery/versions/v0.2.0.md\n', encoding='utf-8'
        )

    def test_release_body_has_user_content_and_omits_audit(self):
        notes = release_notes.generate(self.root, self.tag)
        self.assertIn('下载校验', notes)
        for text in ['工程审计', '比较链接', '返回版本总览', '> 状态']:
            self.assertNotIn(text, notes)

    def test_rejects_version_and_lock_mismatch(self):
        with self.assertRaises(ValueError):
            release_notes.generate(self.root, 'v0.3.0')
        (self.root / 'Cargo.lock').write_text('[[package]]\nname="tiny-md"\nversion="0.1.0"\n')
        with self.assertRaises(ValueError):
            release_notes.generate(self.root, self.tag)

    def test_requires_detail_index_summary_and_link(self):
        for file in [self.detail, self.detail.parent / 'index.md', self.root / 'CHANGELOG.md']:
            content = file.read_text(encoding='utf-8')
            file.write_text('', encoding='utf-8')
            with self.assertRaises(ValueError):
                release_notes.generate(self.root, self.tag)
            file.write_text(content, encoding='utf-8')

    def test_rejects_empty_overview_duplicate_and_invalid_dates(self):
        text = self.detail.read_text(encoding='utf-8')
        for invalid in [text.replace('支持在线更新。', ''), text.replace('2026-10-08', '2026-02-30'), text + '\n## 版本概述\n重复\n']:
            self.detail.write_text(invalid, encoding='utf-8')
            with self.assertRaises(ValueError):
                release_notes.generate(self.root, self.tag)

    def test_semver_tags_are_strict(self):
        for bad in ['v01.2.3', 'v1.2', 'v1.2.3-beta.01', '1.2.3', 'v1.2.3-']:
            with self.assertRaises(ValueError, msg=bad):
                release_notes.parse_tag(bad)
        self.assertEqual(release_notes.parse_tag('v1.2.3-beta.1'), '1.2.3-beta.1')

    def test_rejects_duplicate_index_versions_and_stale_current_version(self):
        index = self.detail.parent / 'index.md'
        text = index.read_text(encoding='utf-8')
        for wrong in [text + '| [v0.2.0](v0.2.0.md) | 2026-10-07 | 已发布 | 重复 |\n', text.replace('当前代码版本为 `0.2.0`', '当前代码版本为 `0.1.0`')]:
            index.write_text(wrong, encoding='utf-8')
            with self.assertRaises(ValueError):
                release_notes.generate(self.root, self.tag)

    def prepare_assets(self):
        dist = self.root / 'dist'
        dist.mkdir()
        for name in update_manifest.expected_names(self.tag):
            (dist / name).write_bytes(b'installer payload')
        (dist / 'release-notes.md').write_text('版本说明', encoding='utf-8')
        return dist

    def test_manifest_digest_size_and_exact_asset_set(self):
        dist = self.prepare_assets()
        manifest = update_manifest.generate(dist, self.tag)
        self.assertEqual(manifest['version'], self.tag)
        self.assertEqual(manifest['schema_version'], 1)
        self.assertEqual(len(manifest['assets']), 4)
        for asset in manifest['assets']:
            self.assertEqual(asset['digest'], 'sha256:' + hashlib.sha256(b'installer payload').hexdigest())
            self.assertEqual(asset['size'], len(b'installer payload'))
            self.assertIn('/tiny-md/releases/download/v0.2.0/', asset['url'])
        self.assertTrue(manifest['notes_url'].endswith('/release-notes.md'))
        json.dumps(manifest)

    def test_manifest_rejects_missing_empty_extra_or_symlink_assets(self):
        dist = self.prepare_assets()
        one = dist / next(iter(update_manifest.expected_names(self.tag)))
        one.write_bytes(b'')
        with self.assertRaises(ValueError):
            update_manifest.generate(dist, self.tag)
        one.unlink()
        with self.assertRaises(ValueError):
            update_manifest.generate(dist, self.tag)
        one.write_bytes(b'installer payload')
        (dist / 'unexpected.exe').write_bytes(b'wrong version')
        with self.assertRaises(ValueError):
            update_manifest.generate(dist, self.tag)

    def test_manifest_requires_release_notes(self):
        dist = self.prepare_assets()
        (dist / 'release-notes.md').unlink()
        with self.assertRaises(ValueError):
            update_manifest.generate(dist, self.tag)

    def test_public_release_with_changed_assets_cannot_be_overwritten(self):
        dist = self.prepare_assets()
        (dist / 'SHA256SUMS').write_text('current checksum')
        (dist / 'update-manifest.json').write_text('{}')
        calls = []
        def fake_gh(*args, **kwargs):
            calls.append(args)
            if args[:2] == ('release', 'view'):
                return subprocess.CompletedProcess(args, 0, '{"isDraft":false}')
            if args[:2] == ('release', 'download'):
                Path(args[args.index('--dir') + 1], 'SHA256SUMS').write_text('old checksum')
                return subprocess.CompletedProcess(args, 0, '')
            self.fail(f'Public release must not be mutated: {args}')
        with patch.object(publish_release, 'gh', side_effect=fake_gh):
            with self.assertRaises(ValueError):
                publish_release.publish(self.tag, dist)
        self.assertFalse(any('upload' in call or 'edit' in call for call in calls))

    def test_release_only_publishes_after_successful_asset_upload(self):
        dist = self.prepare_assets()
        (dist / 'SHA256SUMS').write_text('checksum')
        (dist / 'update-manifest.json').write_text('{}')
        calls = []
        def fake_gh(*args, **kwargs):
            calls.append(args)
            if args[-1] == 'assets':
                body = {'assets': [{'name': p.name, 'size': p.stat().st_size} for p in dist.iterdir()]}
                return subprocess.CompletedProcess(args, 0, json.dumps(body))
            if args[:2] == ('release', 'view'):
                return subprocess.CompletedProcess(args, 1, '')
            return subprocess.CompletedProcess(args, 0, '')
        with patch.object(publish_release, 'gh', side_effect=fake_gh):
            publish_release.publish(self.tag, dist)
        self.assertIn('--draft', calls[1])
        self.assertEqual(calls[2][1], 'upload')
        self.assertEqual(calls[3][-1], 'assets')
        self.assertEqual(calls[4][1], 'edit')
        self.assertIn('--draft=false', calls[4])

    def test_incomplete_remote_assets_are_never_published(self):
        dist = self.prepare_assets()
        (dist / 'SHA256SUMS').write_text('checksum')
        (dist / 'update-manifest.json').write_text('{}')
        calls = []
        def fake_gh(*args, **kwargs):
            calls.append(args)
            if args[-1] == 'assets':
                return subprocess.CompletedProcess(args, 0, '{"assets":[]}')
            if args[1] == 'view':
                return subprocess.CompletedProcess(args, 1, '')
            return subprocess.CompletedProcess(args, 0, '')
        with patch.object(publish_release, 'gh', side_effect=fake_gh):
            with self.assertRaises(ValueError):
                publish_release.publish(self.tag, dist)
        self.assertFalse(any(call[1] == 'edit' for call in calls))

    def test_upload_failure_keeps_release_in_draft(self):
        dist = self.prepare_assets()
        (dist / 'SHA256SUMS').write_text('checksum')
        (dist / 'update-manifest.json').write_text('{}')
        calls = []
        def fake_gh(*args, **kwargs):
            calls.append(args)
            if args[1] == 'view':
                return subprocess.CompletedProcess(args, 1, '')
            if args[1] == 'upload':
                raise subprocess.CalledProcessError(1, args)
            return subprocess.CompletedProcess(args, 0, '')
        with patch.object(publish_release, 'gh', side_effect=fake_gh):
            with self.assertRaises(subprocess.CalledProcessError):
                publish_release.publish(self.tag, dist)
        self.assertFalse(any(call[1] == 'edit' for call in calls))


if __name__ == '__main__':
    unittest.main()
