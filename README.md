# kota 🦎

A TUI agent coder & computer assistant that runs on local LLMs.

```
   _  __      _
  | |/ /___  | |_  __ _   🦎
  | ' // _ \ | __|/ _` |
  | . \ (_) || |_| (_| |
  |_|\_\___/  \__|\__,_|
   [ LOCAL AI CO-PILOT ]

╭─ kota ──────────────────────────────────────────────╮
│  ▶ read the main.rs file and add error handling     │
│  ── step 1 (342 tokens) ──                          │
│  💭 I need to read the file first...                │
│  🔧 read_file({"path": "src/main.rs"})              │
│  ✓ read_file (12ms)                                 │
│    → use std::io;                                   │
│  ── step 2 (1204 tokens) ──                         │
│  Here's the updated file with error handling:       │
│  🔧 write_file({"path": "src/main.rs", ...})        │
│  ✓ write_file (3ms)                                 │
│  ── done (2340ms) ──                                │
╰─────────────────────────────────────────────────────╯
```

## What it does

- Talks to any OpenAI-compatible local LLM (Ollama, llama-server, RamaLama, vLLM)
- 5 built-in tools: read_file, write_file, list_dir, run_command, search
- Streams tokens and thinking/reasoning traces in real time
- Shows tool calls as they happen with timing
- Tracks context budget so you don't silently overflow
- Single binary, no Python, no Node.js

## Quick start

```bash
# 1. Start a local model
ollama serve
ollama pull qwen3:8b

# 2. Build and run kota
cargo run

# Or with options:
cargo run -- --model qwen3:8b --api-url http://localhost:11434/v1 --workdir ~/myproject
```

## Keybindings

| Key         | Action             |
| ----------- | ------------------ |
| Enter       | Send message       |
| Ctrl+C      | Quit               |
| Ctrl+R      | Reset conversation |
| PageUp/Down | Scroll output      |

## Architecture

```
┌──────────┐     ┌───────────┐     ┌──────────────────────────┐
│  TUI     │◄───►│  Agent    │◄───►│  Local LLM               │
│ (ratatui)│     │  Loop     │     │  Ollama / llama-server / |
└──────────┘     └─────┬─────┘     │  vLLM (for Nemotron)     │
                       │           └──────────────────────────┘
                 ┌─────▼─────┐
                 │  Tools    │
                 │ read_file │
                 │ write_file│
                 │ list_dir  │
                 │ run_cmd   │
                 │ search    │
                 └───────────┘
```

The agent loop:

1. Build context (system prompt + conversation history)
2. Stream completion from local model
3. If model emits a tool call → execute it, append result, go to 1
4. If model emits a final message → done

All events flow through a typed channel. The TUI subscribes and renders them.

## Models tested

| Model               | Backend | Architecture                  | Status |
| ------------------- | ------- | ----------------------------- | ------ |
| qwen3:8b            | Ollama  | Dense, GQA                    | ✓      |
| qwen3.6:27b         | Ollama  | Dense Transformer             | target |
| gemma4:e4b          | Ollama  | Dense Transformer             | target |
| gemma4:31b          | Ollama  | Dense Transformer             | target |
| nemotron3-nano-omni | vLLM    | Mamba2-Transformer Hybrid MoE | target |

### A note on Qwen 3 and Quantization

Recent research indicates that the Qwen 3 family (e.g., Qwen3-8B) utilizes a Dense architecture with Grouped-Query Attention (GQA) that exhibits strong, consistent gate structure. Because its MLP layers are highly structured, Qwen 3 is uniquely amenable to aggressive 4-bit block quantization. For local execution with `kota`, using 4-bit quantized GGUF variants (like `q4_K_M`) will deliver near-unquantized reasoning performance while drastically reducing your memory footprint.

### A note on Gemma

Google's Gemma family (specifically Gemma 3 4B/27B and Gemma 4 architectures) are highly optimized for local agentic execution. Due to advanced architecture designs (such as grouped-query attention and sliding window mechanisms), Gemma models deliver class-leading reasoning and tool-calling capabilities at lower parameter counts. For consumer devices like an M1 Pro Mac, a quantized Gemma model (such as Gemma 3 9B) is highly recommended for coding and research tasks, offering excellent accuracy without exhausting the 16GB Unified Memory buffer.

### A note on Nemotron 3 Nano Omni

NVIDIA's Nemotron 3 Nano Omni (30B-A3B) is a multimodal model that processes video, audio, images, and text through a unified Mamba2-Transformer hybrid MoE architecture. It activates ~3B parameters per token (same as Qwen 3.6-35B-A3B) but uses a fundamentally different backbone — selective state space models (Mamba2) for some layers, attention for others.

Nemotron requires NVIDIA GPU hardware and runs best via vLLM or TensorRT-LLM (not yet available as GGUF for llama.cpp/Ollama). To benchmark it alongside the other models through kota, serve it via vLLM on an NVIDIA GPU and point kota at the vLLM endpoint:

```bash
# Serve Nemotron via vLLM (requires NVIDIA GPU)
vllm serve nvidia/Nemotron-3-Nano-Omni-30B-A3B-Reasoning-BF16

# Point kota at it
cargo run -- --model Nemotron-3-Nano-Omni-30B-A3B-Reasoning-BF16 \
  --api-url http://localhost:8000/v1 --workdir ~/myproject
```


