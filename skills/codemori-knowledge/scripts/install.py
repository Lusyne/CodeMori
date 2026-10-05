"""Install this Skill plus its matching native CLI into a discoverable local skills folder."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import uuid
from codemori_tool import executable


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli')
    parser.add_argument('--destination', type=Path, default=Path(os.environ.get('CODEX_HOME', str(Path.home() / '.codex'))) / 'skills/codemori-knowledge')
    parser.add_argument('--replace', action='store_true', help='Keep the previous installation in a sibling skill-backups directory')
    args = parser.parse_args()
    destination = args.destination.expanduser().resolve()
    source = Path(__file__).resolve().parents[1]
    if destination == source or source in destination.parents or destination in source.parents: raise ValueError('Install into a different directory from the source skill')
    if destination.exists() and not args.replace: raise ValueError('Skill already exists; inspect it and use --replace to preserve a backup')
    binary = Path(executable(args.cli))
    info = json.loads(subprocess.check_output([str(binary), 'info'], encoding='utf-8', timeout=10))
    if info.get('protocol_version') != 1 or info.get('ok') is not True: raise ValueError('Invalid CodeMori core')
    version = info['data']['version']; parts = version.split('.')
    if (int(parts[0]), int(parts[1])) < (0, 1): raise ValueError('This Skill requires CodeMori 0.1.0 or newer')
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.codemori-install-', dir=destination.parent) as staging:
        staged = Path(staging) / 'skill'
        shutil.copytree(source, staged, ignore=shutil.ignore_patterns('bin', '__pycache__', '*.pyc', 'installation.json'))
        for name in ['LICENSE']:
            candidates = [binary.parent / name, binary.parent.parent / name, source / name, source.parents[1] / name, source.parents[1] / 'docs' / name]
            notice = next((p for p in candidates if p.is_file()), None)
            if notice is None: raise ValueError('Use a complete CodeMori CLI distribution with ' + name)
            shutil.copy2(notice, staged / name)
        (staged / 'bin').mkdir()
        installed_binary = staged / 'bin' / ('codemori.exe' if os.name == 'nt' else 'codemori')
        shutil.copy2(binary, installed_binary); installed_binary.chmod(0o755)
        (staged / 'installation.json').write_text(json.dumps({'version':version,'binary_sha256':hashlib.sha256(installed_binary.read_bytes()).hexdigest()}, indent=2) + '\n', encoding='utf-8')
        backup = None
        if destination.exists():
            backup = destination.parent.parent / 'skill-backups' / ('codemori-knowledge-' + uuid.uuid4().hex)
            backup.parent.mkdir(parents=True, exist_ok=True); destination.rename(backup)
        try: staged.rename(destination)
        except OSError:
            if backup is not None: backup.rename(destination)
            raise
    print(json.dumps({'installed':str(destination),'version':version,'previous_backup':str(backup) if backup else None}, ensure_ascii=False))


if __name__ == '__main__':
    main()
