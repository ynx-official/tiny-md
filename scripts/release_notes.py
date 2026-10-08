"""Validate release documentation and emit the single user-facing release body."""

import argparse
from datetime import date
from pathlib import Path
import re
import sys
import tomllib

WORKSPACE_PACKAGES = ('tiny-md', 'tiny-md-editor', 'tiny-md-document', 'tiny-md-updater')
TAG = re.compile(r'v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z.-]+))?(?:\+([0-9A-Za-z.-]+))?\Z')


def parse_tag(tag):
    """Reject non-SemVer identifiers, including numeric prerelease leading zeroes."""
    match = TAG.fullmatch(tag)
    if not match:
        raise ValueError(f'无效版本标签：{tag}')
    for group in (match[4], match[5]):
        if group is not None and any(not part for part in group.split('.')):
            raise ValueError(f'无效版本标识：{tag}')
    if match[4] and any(part.isdigit() and len(part) > 1 and part[0] == '0' for part in match[4].split('.')):
        raise ValueError(f'预发布数字不能有前导零：{tag}')
    return tag[1:]


def current_version(root):
    return tomllib.loads((root / 'Cargo.toml').read_text(encoding='utf-8'))['workspace']['package']['version']


def visible_notes(detail):
    """Keep product and upgrade sections; omit audit evidence and local navigation."""
    result = []
    skipping = False
    for line in detail.splitlines():
        if line.startswith('## '):
            skipping = line[3:].strip() in {'验证结果', '变更依据'}
        if not skipping and not line.startswith(('[返回版本总览]', '> 状态', '> 最后更新', '> 关联文档')):
            result.append(line)
    return '\n'.join(result).strip() + '\n'


def generate(root, tag):
    """Fail before building if tag, lockfile, detail or indexes disagree."""
    version = parse_tag(tag)
    if version != current_version(root):
        raise ValueError('标签与 Cargo.toml 的 workspace.package.version 不一致')
    lock = tomllib.loads((root / 'Cargo.lock').read_text(encoding='utf-8'))
    for name in WORKSPACE_PACKAGES:
        packages = [p for p in lock.get('package', []) if p['name'] == name and 'source' not in p]
        if len(packages) != 1 or packages[0]['version'] != version:
            raise ValueError(f'Cargo.lock 的 {name} 版本与标签不一致')
    detail_path = root / f'docs/06-delivery/versions/{tag}.md'
    try:
        detail = detail_path.read_text(encoding='utf-8')
        overview = (detail_path.parent / 'index.md').read_text(encoding='utf-8')
        changelog = (root / 'CHANGELOG.md').read_text(encoding='utf-8')
    except OSError as error:
        raise ValueError(f'缺少发布资料：{error}') from error
    if detail.splitlines()[:1] != [f'# Tiny MD {tag}']:
        raise ValueError('版本详情标题与标签不一致')
    dates = re.findall(r'^> 发布日期：(\d{4}-\d{2}-\d{2})$', detail, re.MULTILINE)
    if len(dates) != 1:
        raise ValueError('版本详情必须有唯一发布日期')
    date.fromisoformat(dates[0])
    overview_parts = re.findall(r'^## 版本概述\s*\n(.*?)(?=^## |\Z)', detail, re.MULTILINE | re.DOTALL)
    if len(overview_parts) != 1 or not overview_parts[0].strip():
        raise ValueError('版本概述缺失、为空或重复')
    header = f'## [{version}] - {dates[0]}'
    if changelog.splitlines().count(header) != 1 or sum(line.startswith(f'## [{version}] - ') for line in changelog.splitlines()) != 1:
        raise ValueError('CHANGELOG 版本日期条目缺失或重复')
    if changelog.splitlines().count(f'[{version}]: docs/06-delivery/versions/{tag}.md') != 1:
        raise ValueError('CHANGELOG 必须链接唯一版本详情')
    prefix = f'| [{tag}]({tag}.md) | {dates[0]} |'
    if sum(line.startswith(prefix) for line in overview.splitlines()) != 1 or sum(line.startswith(f'| [{tag}](') for line in overview.splitlines()) != 1:
        raise ValueError('版本总览条目与详情不一致')
    if re.findall(r'当前代码版本为 `([^`]+)`', overview) != [version]:
        raise ValueError('版本总览的当前代码版本与标签不一致')
    return visible_notes(detail)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument('--tag')
    source.add_argument('--check-current', action='store_true')
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    try:
        tag = args.tag or 'v' + current_version(args.root)
        notes = generate(args.root, tag)
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(notes, encoding='utf-8', newline='\n')
        print(f'发布资料校验通过：{tag}')
        return 0
    except (ValueError, OSError, KeyError) as error:
        print(f'发布资料校验失败：{error}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
