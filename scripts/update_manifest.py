"""Generate schema-1 update metadata from the complete Tiny MD release assets."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys
from urllib.parse import quote

from release_notes import parse_tag

REPOSITORY = 'ynx-official/tiny-md'


def expected_names(tag):
    parse_tag(tag)
    return {
        f'tiny-md-{tag}-windows-x64-setup.exe',
        f'tiny-md-{tag}-windows-x64-portable.zip',
        f'tiny-md-{tag}-macos-arm64.dmg',
        f'tiny-md-{tag}-macos-arm64-portable.zip',
    }


def generate(dist, tag, repository=REPOSITORY):
    """Require all platform editions and exact names; hash files without buffering."""
    expected = expected_names(tag)
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository):
        raise ValueError('无效仓库标识')
    files = {p.name: p for p in dist.iterdir()}
    if any(p.is_symlink() or not p.is_file() for p in files.values()):
        raise ValueError('发布产物不能是目录或符号链接')
    metadata = {'release-notes.md', 'SHA256SUMS', 'update-manifest.json'}
    actual = set(files) - metadata
    if actual != expected:
        raise ValueError(f'发布产物不完整：缺少 {expected - actual}，多余 {actual - expected}')
    if 'release-notes.md' not in files or not files['release-notes.md'].read_text(encoding='utf-8').strip():
        raise ValueError('缺少非空 release-notes.md')
    base = f'https://github.com/{repository}/releases/download/{quote(tag, safe="")}'
    assets = []
    for name in sorted(expected):
        path = files[name]
        size = path.stat().st_size
        if size <= 0 or size > 1024 * 1024 * 1024:
            raise ValueError(f'更新包大小无效：{name}')
        with path.open('rb') as content:
            digest = hashlib.file_digest(content, 'sha256').hexdigest()
        assets.append({'name': name, 'url': f'{base}/{quote(name, safe="")}', 'size': size, 'digest': f'sha256:{digest}'})
    return {'schema_version': 1, 'version': tag, 'notes_url': f'{base}/release-notes.md', 'assets': assets}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dist', type=Path, required=True)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--repository', default=REPOSITORY)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        manifest = generate(args.dist, args.tag, args.repository)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n', encoding='utf-8', newline='\n')
        print(f'更新清单已生成：{args.tag}，{len(manifest["assets"])} 个产物')
        return 0
    except (ValueError, OSError) as error:
        print(f'更新清单生成失败：{error}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
