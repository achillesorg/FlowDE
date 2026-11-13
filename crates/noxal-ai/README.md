# noxal-ai

NoxalAi: local Qwen assistant service. Runtime trait (mock, llama.cpp CLI), model registry (no downloads), RAM/thermal/battery gating, idle unload, ChatML prompts, private conversation store (optional encryption), safe tool layer with per-tool permissions, streaming events and D-Bus `org.flowde.Noxal1`.

**Status:** Everything except a real model run is unit tested with the mock backend; no real model was run here.

## TODO(feature)

- llama.cpp server backend over a loopback socket
- In-process backend (bindings) with real unload
- Multi-step tool loop (tool results fed back)
- Speech in/out hooks
- Retrieval over local data (calendar, notes) with consent
- Model download helper (explicit, user-initiated)
- Context summarisation
