"""Exercise 0.3 -> 0.4 compatibility using an actual archived old core and isolated data."""
import hashlib
import json
from pathlib import Path
import platform
import sqlite3
import subprocess
import sys
import tarfile
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
archive = Path(sys.argv[1]).resolve()
assert hashlib.sha256(archive.read_bytes()).hexdigest() == archive.with_name(archive.name + '.sha256').read_text().split()[0]
filename = 'codemori.exe' if platform.system() == 'Windows' else 'codemori'
if archive.suffix == '.zip':
    with zipfile.ZipFile(archive) as z:
        name = next(n for n in z.namelist() if n.endswith('/' + filename)); old_bytes = z.read(name)
else:
    with tarfile.open(archive) as z:
        member = next(m for m in z.getmembers() if m.name.endswith('/' + filename)); old_bytes = z.extractfile(member).read()
with tempfile.TemporaryDirectory(prefix='context-upgrade-', dir=ROOT / 'target') as temp:
    qa = Path(temp); old = qa / filename; old.write_bytes(old_bytes); old.chmod(0o755)
    current = ROOT / 'target/release' / filename; data = qa / 'store'; project = qa / 'project'; project.mkdir()
    (project / 'A.java').write_text('class A {}\n')
    def rpc(binary, request, allow_error=False):
        completed = subprocess.run([str(binary), 'rpc', '--data-dir', str(data)], input=json.dumps({'protocol_version':1,'request':request}), text=True, capture_output=True, timeout=30)
        result = json.loads(completed.stdout)
        if allow_error: return result
        assert completed.returncode == 0 and result['ok'], result
        return result['data']
    rpc(old, {'op':'identity_set','display_name':'Upgrade QA'})
    workspace = rpc(old, {'op':'workspace_register','root':str(project)})
    document = rpc(old, {'op':'record_create','record':{'kind':'document','title':'Legacy design','url':'https://example.com/legacy'}})
    rpc(old, {'op':'document_link','binding':{'workspace_id':workspace['id'],'path':'A.java','document_id':document['id']}})
    shared = rpc(old, {'op':'project_save','root':str(project),'expected_version':'missing','record':{'kind':'snippet','content':'legacy();'}})
    before = rpc(old, {'op':'backup_export'}); assert before['format_version'] == 1
    migrated = rpc(current, {'op':'backup_export'}); assert migrated['format_version'] == 2 and migrated['records'] == before['records'] and migrated['bindings'] == before['bindings']
    backups = list(data.glob('migration-v3-*.sqlite3.bak')); assert len(backups) == 1
    with sqlite3.connect(backups[0]) as database: assert database.execute('PRAGMA user_version').fetchone()[0] == 3
    rejected = rpc(old, {'op':'search'}, True); assert not rejected['ok'] and rejected['error']['code'] == 'SCHEMA_UNSUPPORTED'
    rpc(current, {'op':'project_save','root':str(project),'expected_version':shared['project_version'],'id':shared['id'],'revision':shared['revision'],'record':{'kind':'snippet','content':'updated();'}})
    assert rpc(old, {'op':'project_info','root':str(project)})['error']
    print(json.dumps({'old_core':archive.name,'personal_schema3_to4':True,'online_backup_schema3':True,'legacy_records_and_bindings_preserved':True,'backup1_to2':True,'old_core_refuses_schema4':True,'old_core_refuses_shared3':True}))
