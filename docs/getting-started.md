# Getting Started

This guide will walk you through setting up and running Kota locally.

## Installation

First, make sure you have Rust and Cargo installed. Clone the repository and run:

```bash
cargo build --release
```

To install the documentation viewer locally, run:

```bash
cargo install mdbook
```

## Running the Agent

Start your local LLM engine (e.g., Ollama, llama.cpp, or vLLM). Then run Kota:

```bash
cargo run -- --model qwen3:8b --api-url http://localhost:11434/v1 --workdir ~/my-project
```

## Hotkeys

- **Enter**: Send message
- **Ctrl+C**: Quit
- **Ctrl+R**: Reset conversation
- **PageUp/PageDown**: Scroll output
