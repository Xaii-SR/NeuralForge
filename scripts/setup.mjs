import { execSync } from 'child_process';
import { platform } from 'os';
import * as fs from 'fs';

console.log("==================================================");
console.log("NEURAL FORGE: UNIFIED SETUP & AI DEPLOYMENT");
console.log("==================================================");

const osPlatform = platform();

function runCommand(command, silent = false) {
    try {
        execSync(command, { stdio: silent ? 'ignore' : 'inherit' });
        return true;
    } catch (error) {
        return false;
    }
}

function checkOllamaInstalled() {
    console.log("[*] Checking for Ollama engine...");
    return runCommand('ollama --version', true);
}

function installOllama() {
    console.log("[!] Ollama not found. Initiating silent installation...");
    if (osPlatform === 'darwin' || osPlatform === 'linux') {
        runCommand('curl -fsSL https://ollama.com/install.sh | sh');
    } else if (osPlatform === 'win32') {
        console.log("[!] Please install Ollama from https://ollama.com/download, or wait for winget to attempt installation.");
        runCommand('winget install -e --id Ollama.Ollama');
    } else {
        console.error("[-] Unsupported OS for automated Ollama installation.");
        process.exit(1);
    }
}

function ensureOllamaRunning() {
    console.log("[*] Verifying Ollama daemon is active...");
    try {
        const response = fetch('http://127.0.0.1:11434/api/tags');
        console.log("[+] Ollama daemon is running.");
    } catch (e) {
        console.log("[!] Starting Ollama daemon in the background...");
        if (osPlatform === 'darwin') {
            runCommand('open -a Ollama', true);
        } else if (osPlatform === 'win32') {
            runCommand('start ollama serve', true);
        } else {
            runCommand('ollama serve &', true);
        }
        execSync(osPlatform === 'win32' ? 'timeout 3' : 'sleep 3');
    }
}

function pullModels() {
    console.log("[*] Acquiring required local AI models...");
    console.log("[*] Pulling Qwen...");
    runCommand('ollama pull qwen2.5-coder:1.5b');
    console.log("[*] Pulling DeepSeek...");
    runCommand('ollama pull deepseek-coder:latest');
}

function main() {
    if (!checkOllamaInstalled()) {
        installOllama();
    } else {
        console.log("[+] Ollama is already installed.");
    }

    ensureOllamaRunning();
    pullModels();

    console.log("==================================================");
    console.log("[+] SETUP COMPLETE. Neural Forge is ready to launch.");
    console.log("==================================================");
}

main();