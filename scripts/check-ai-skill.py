"""Validate the shipped Skill bridge against actual core data in a disposable project."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
cli = ROOT / 'target/release' / ('codemori.exe' if sys.platform == 'win32' else 'codemori')
helper = ROOT / 'skills/codemori-knowledge/scripts/codemori_tool.py'
with tempfile.TemporaryDirectory(prefix='codemori-skill-', dir=ROOT / 'target') as directory:
    qa = Path(directory); project = qa / 'project'; store = qa / 'store'; (project / 'src').mkdir(parents=True)
    source = project / 'src/pay.py'; source.write_text('def retry_payment(error):\n    return True\n')
    def rpc(request):
        result = subprocess.run([str(cli), 'rpc', '--data-dir', str(store)], input=json.dumps({'protocol_version':1,'request':request}), text=True, encoding='utf-8', capture_output=True, check=True)
        value = json.loads(result.stdout); assert value['ok']; return value['data']
    workspace = rpc({'op':'workspace_register','root':str(project)})
    doc = rpc({'op':'record_create','record':{'kind':'document','title':'Retry design','description':'All failures retry.','url':'https://example.com/retry'}})
    rpc({'op':'document_link','binding':{'workspace_id':workspace['id'],'path':'src/pay.py','document_id':doc['id']}})
    source.write_text("def retry_payment(error):\n    return error == 'timeout'\n")
    before = rpc({'op':'backup_export'})
    def tool(command, *args, success=True):
        result = subprocess.run([sys.executable, str(helper), '--cli', str(cli), '--data-dir', str(store), command, '--root', str(project), *args], text=True, encoding='utf-8', capture_output=True)
        data = json.loads(result.stdout)
        assert (result.returncode == 0) == success, result.stdout + result.stderr
        return data
    context = tool('context', '--query', 'retry', '--file', 'src/pay.py')['data']
    assert context['external_document_bodies_loaded'] is False and context['review_confirmation_performed'] is False
    assert context['associations']['entries'][0]['review_state']['status'] == 'needs_review'
    assert context['knowledge']['items'][0]['record']['input']['description'] == 'All failures retry.'
    code = tool('read', '--file', 'src/pay.py', '--start', '1', '--end', '2')['data']
    assert "return error == 'timeout'" in code['content'] and code['start_line'] == 1
    search = tool('search', '--query', 'retry_payment', '--under', 'src')['data']; assert search['hits'][0]['path'] == 'src/pay.py'
    link = tool('link', '--file', 'src/pay.py', '--line', '1')['data']; assert link['vscode_url'].startswith('vscode://')
    tool('read', '--file', '../outside.py', success=False)
    after = rpc({'op':'backup_export'}); assert before['records'] == after['records'] and before['bindings'] == after['bindings']
    assert rpc({'op':'library_file_documents','root':str(project),'workspace_id':workspace['id'],'path':'src/pay.py'})['entries'][0]['review_state']['status'] == 'needs_review'
    installed = qa / 'installed-skill'
    subprocess.run([sys.executable, str(helper.with_name('install.py')), '--cli', str(cli), '--destination', str(installed)], check=True, capture_output=True)
    assert (installed / 'LICENSE').is_file()
    installed_read = subprocess.run([sys.executable, str(installed / 'scripts/codemori_tool.py'), '--data-dir', str(store), 'read', '--root', str(project), '--file', 'src/pay.py', '--end', '2'], text=True, encoding='utf-8', capture_output=True, check=True)
    assert "return error == 'timeout'" in json.loads(installed_read.stdout)['data']['content']
    print(json.dumps({'installed_skill_with_bundled_core':True,'helper_context':True,'current_source_read':True,'stale_summary_distinguished':True,'source_search':True,'portable_links':True,'traversal_refused':True,'records_and_reviews_unchanged':True,'model_generation_evaluated':False}))
