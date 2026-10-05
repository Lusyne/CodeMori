"""Small argv/stdin bridge to CodeMori's bounded knowledge tools; no SQLite or shell access."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys


def executable(explicit):
    if explicit or os.environ.get('CODEMORI_CLI'):
        value = explicit or os.environ['CODEMORI_CLI']
        candidate = Path(value).expanduser()
        if candidate.is_file(): return str(candidate.resolve())
        found = shutil.which(value)
        if found: return found
        raise ValueError('The selected CodeMori CLI does not exist')
    skill = Path(__file__).resolve().parents[1]
    root = skill.parents[1]
    name = 'codemori.exe' if os.name == 'nt' else 'codemori'
    for candidate in [skill / 'bin' / name, root / name, root / 'target/release' / name]:
        if candidate.is_file(): return str(candidate)
    found = shutil.which(name)
    if found: return found
    raise ValueError('Install the matching CodeMori CLI or set CODEMORI_CLI / --cli')


def main():
    if hasattr(sys.stdout, 'reconfigure'): sys.stdout.reconfigure(encoding='utf-8')
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', help='Explicit CodeMori native executable')
    parser.add_argument('--data-dir', help='Explicit personal data directory, normally omitted')
    commands = parser.add_subparsers(dest='command', required=True)
    for name in ['context', 'search', 'read', 'link']:
        command = commands.add_parser(name)
        command.add_argument('--root', required=True)
        if name in ['context', 'search']: command.add_argument('--query', default='' if name == 'context' else None, required=name == 'search')
        if name in ['context', 'read', 'link']: command.add_argument('--file', required=name != 'context')
        if name == 'search':
            command.add_argument('--under', default='.')
            command.add_argument('--limit', type=int, default=20)
        if name == 'read':
            command.add_argument('--start', type=int, default=1)
            command.add_argument('--end', type=int)
        if name == 'link': command.add_argument('--line', type=int, required=True)
    args = parser.parse_args()
    request = {'root': str(Path(args.root).expanduser().resolve())}
    if args.command == 'context': request.update(op='ai_context', query=args.query, path=args.file)
    elif args.command == 'search': request.update(op='code_search', query=args.query, under=args.under, limit=args.limit)
    elif args.command == 'read': request.update(op='code_read', path=args.file, start_line=args.start, end_line=args.end)
    else: request.update(op='code_link_create', path=args.file, line=args.line)
    argv = [executable(args.cli), 'rpc']
    if args.data_dir: argv += ['--data-dir', str(Path(args.data_dir).expanduser().resolve())]
    response = subprocess.run(argv, input=json.dumps({'protocol_version': 1, 'request': request}, ensure_ascii=False), encoding='utf-8', capture_output=True, timeout=30)
    result = json.loads(response.stdout)
    if not isinstance(result, dict) or result.get('protocol_version') != 1 or not isinstance(result.get('ok'), bool): raise ValueError('Invalid CodeMori response; update the core and skill together')
    if result['ok'] and not isinstance(result.get('data'), dict): raise ValueError('Missing CodeMori tool result data')
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if result['ok'] and response.returncode == 0 else 1


if __name__ == '__main__':
    try: sys.exit(main())
    except (ValueError, OSError, subprocess.TimeoutExpired) as error:
        print(json.dumps({'ok': False, 'error': {'code': 'TOOL_ERROR', 'message': str(error)}}, ensure_ascii=False))
        sys.exit(1)
