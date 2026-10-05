const { version } = require('../package.json');
const { execFileSync } = require('node:child_process');
const path = require('node:path');
execFileSync(process.execPath,[path.join(__dirname,'build-native.cjs')],{stdio:'inherit'});
const target = `${process.platform === 'win32' ? 'win32' : process.platform}-${process.arch}`;
const releaseFlags = version.startsWith('0.') || version.includes('-') ? ['--pre-release'] : [];
const vsce = require.resolve('@vscode/vsce/vsce');
execFileSync(process.execPath,[vsce,'package',...releaseFlags,'--target',target,'--no-dependencies','--allow-missing-repository','--out',`codemori-${version}-${target}.vsix`],{cwd:path.resolve(__dirname,'..'),stdio:'inherit'});
