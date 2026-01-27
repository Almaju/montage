# Montage 🎬

AI-powered video editor — feed audio, generate YouTube-ready videos.

## Vision

Montage takes your audio recording and automatically generates a polished video:
- 🎙️ **Audio-first workflow**: Start with a voiceover or podcast
- 🔍 **Smart footage matching**: AI finds relevant B-roll (mention "Canada" → shows Canada footage)
- ✨ **Auto effects**: Zooms, transitions, dark backgrounds synced to your voice
- 🖥️ **Real-time preview**: Desktop app with instant visualization
- 💬 **Prompt-based editing**: Describe changes in natural language

## Tech Stack

- **UI**: [gpui](https://github.com/zed-industries/zed/tree/main/crates/gpui) (Rust GPU-accelerated UI from Zed)
- **Video**: GStreamer (native Rust bindings)
- **AI**: Anthropic Claude, OpenAI, Ollama (configurable)
- **Extensions**: Pexels, Pixabay, and future AI-generated footage

## Roadmap

### Phase 1: Foundation (MVP)
- [ ] gpui desktop app shell
- [ ] Load audio file, display waveform
- [ ] Basic timeline UI
- [ ] Single hardcoded video clip plays synced to audio

### Phase 2: AI Integration
- [ ] Transcript generation from audio
- [ ] LLM analyzes transcript, suggests footage timestamps
- [ ] Basic prompt interface

### Phase 3: Video Assembly
- [ ] GStreamer pipeline for compositing
- [ ] Multiple clips, basic cuts
- [ ] Export to file

### Phase 4: Effects & Polish
- [ ] Zoom effects, transitions
- [ ] Dark backgrounds, overlays
- [ ] Real-time preview

### Phase 5: Extensions
- [ ] Royalty-free footage APIs (Pexels, Pixabay)
- [ ] Multiple LLM backends
- [ ] AI-generated footage (future)

## Getting Started

### Prerequisites
- Rust 1.75+ 
- Linux (macOS and Windows support planned)
- GStreamer (for video processing, Phase 3+)

### Build & Run

```bash
git clone https://github.com/Almaju/montage.git
cd montage
cargo run
```

## Contributing

This is an open source project. Contributions welcome!

1. Fork the repo
2. Create your feature branch (`git checkout -b feature/amazing`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing`)
5. Open a Pull Request

## License

MIT License — see [LICENSE](LICENSE) for details.

## Author

**Alexandre** ([@Almaju](https://github.com/Almaju))

Built with ☕ and Rust.
