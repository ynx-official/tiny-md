"""Publish complete draft assets; a public version is never silently replaced."""

import argparse
import json
from pathlib import Path
import subprocess

from release_notes import parse_tag


def gh(*args, check=True):
    return subprocess.run(['gh', *args], check=check, capture_output=True, text=True)


def publish(tag, assets):
    parse_tag(tag)
    notes = assets / 'release-notes.md'
    sums = assets / 'SHA256SUMS'
    for required in [notes, sums, assets / 'update-manifest.json']:
        if not required.is_file():
            raise ValueError(f'缺少发布产物：{required}')
    existing = gh('release', 'view', tag, '--json', 'isDraft', check=False)
    if existing.returncode == 0 and not json.loads(existing.stdout)['isDraft']:
        # Download only the checksum ledger into a temporary directory. Metadata
        # and release notes are included in it, so changed documentation also fails.
        import tempfile
        with tempfile.TemporaryDirectory() as directory:
            gh('release', 'download', tag, '--pattern', 'SHA256SUMS', '--dir', directory)
            if (Path(directory) / 'SHA256SUMS').read_bytes() != sums.read_bytes():
                raise ValueError('同版本已公开且产物不同：请升级版本，禁止覆盖公开附件')
        print(f'{tag} 已公开，校验和一致，无需重新上传')
        return
    if existing.returncode != 0:
        flags = ['--prerelease'] if '-' in tag.split('+')[0] else []
        gh('release', 'create', tag, '--draft', '--verify-tag', '--title', f'Tiny MD {tag}', '--notes-file', str(notes), *flags)
    gh('release', 'upload', tag, *(str(p) for p in sorted(assets.iterdir()) if p.is_file()), '--clobber')
    remote = json.loads(gh('release', 'view', tag, '--json', 'assets').stdout)['assets']
    expected = {p.name: p.stat().st_size for p in assets.iterdir() if p.is_file()}
    actual = {p['name']: p['size'] for p in remote}
    if actual != expected or len(actual) != len(remote):
        raise ValueError('草稿远端附件集合或大小与本次产物不一致，未公开 Release')
    gh('release', 'edit', tag, '--notes-file', str(notes), '--draft=false')
    print(f'{tag} 所有附件上传完毕，已从草稿发布')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--assets', type=Path, required=True)
    args = parser.parse_args()
    publish(args.tag, args.assets)
