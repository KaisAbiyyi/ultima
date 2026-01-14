# Ultima

Ultima is a professional local AI workspace and agent orchestration platform. Designed to evolve beyond simple inference management, Ultima aims to function as an Agentic OS, enabling complex reasoning, multi-agent debate, and creative generation within a secure, local environment.

## Vision

Ultima focuses on productivity and orchestration. While standard model loaders focus on inference, Ultima is built for work. The platform transitions from a robust chat interface to a sophisticated multi-agent orchestration layer capable of coordinating multiple LLM instances to solve complex tasks.

## Key Features

### Core Management
- **HuggingFace Integration**: Native search, filtering by quantization or size, and integrated download management.
- **Hardware Optimization**: Granular control over GPU layers, CPU threads, and main context size.
- **Local Server API**: OpenAI-compatible endpoint for utilizing Ultima as a backend for external tools.
- **Performance Monitoring**: Real-time metrics including tokens per second, time-to-first-token (TTFT), and VRAM/RAM utilization.
- **State Management**: Save and load system prompts and inference parameters on a per-model or per-task basis.

### Advanced Capabilities
- **Multimodal Support**: Integration for vision models using CLIP projectors.
- **Tool Calling**: Implementation of GBNF grammars for reliable, structured JSON output.
- **Context Management**: Serial execution with state caching and KV cache swapping for efficient multi-agent interaction on single-GPU systems.

## Tech Stack

- **Frontend**: SvelteKit, TypeScript, Tailwind CSS, Flowbite.
- **Backend**: Rust (Tauri framework).
- **Storage**: SQLite for chat history and persistence.
- **Inference Engine**: llama.cpp (llama-server integration).

## Getting Started

### Prerequisites

- [Node.js](https://nodejs.org/) (Latest LTS recommended)
- [Rust](https://www.rust-lang.org/) (Latest stable version)

### Installation

1. Clone the repository:
   ```bash
   git clone https://github.com/your-repo/ultima.git
   cd ultima
   ```

2. Install dependencies:
   ```bash
   npm install
   ```

### Development

To start the application in development mode:
```bash
npm run tauri dev
```

### Build

To create a production-ready bundle:
```bash
npm run tauri build
```

## Contribution Guidelines

Ultima is open for contributions. We welcome pull requests for bug fixes, new features, and documentation improvements.

1. Fork the repository.
2. Create a feature branch.
3. Commit your changes with professional and descriptive messages.
4. Open a pull request with a detailed summary of your changes.

Please ensure your code adheres to the project's architectural standards and includes necessary documentation or tests where applicable.

## License

This project is licensed under the GPL-3.0 License. See the [LICENSE.md](LICENSE.md) file for details.
