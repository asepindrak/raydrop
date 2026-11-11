import { spawn } from 'node:child_process';
import os from 'node:os';
import path from 'node:path';

const cargoBin = path.join(os.homedir(), '.cargo', 'bin');
const sep = process.platform === 'win32' ? ';' : ':';
const currentPath = process.env.PATH || process.env.Path || '';

const updatedEnv = {
  ...process.env,
  PATH: `${cargoBin}${sep}${currentPath}`,
  Path: `${cargoBin}${sep}${currentPath}`,
};

const cliEntry = path.resolve('node_modules', '@tauri-apps', 'cli', 'tauri.js');
const args = [cliEntry, ...process.argv.slice(2)];

const child = spawn(process.execPath, args, {
  stdio: 'inherit',
  env: updatedEnv,
});

child.on('exit', (code) => {
  process.exit(code ?? 0);
});

