# Python client

Start the [echo server](../README.md), then run these commands from this directory
with Python 3.10 or later:

```sh
python -m venv .venv
source .venv/bin/activate
python -m pip install -r requirements.txt
python client.py --host 127.0.0.1 --port 3000
```

The client sends one message over each transport (reliable WebSocket and
unreliable WebRTC), then prints replies until you press **Ctrl+C**.

Arguments:

| Argument | Default | Meaning |
| --- | --- | --- |
| `--host` | `127.0.0.1` | Server hostname or IP, without scheme or path |
| `--port` | `3000` | WebSocket port |
| `--secure` | off | Use `wss` through a TLS WebSocket endpoint |
| `--message` | `Hello from Python!` | UTF-8 text to send as bytes on both transports |
| `--duration` | `0` | Seconds to listen after sending; 0 waits until Ctrl+C |

For a short run:

```sh
python client.py --message "Hello server" --duration 3
```
