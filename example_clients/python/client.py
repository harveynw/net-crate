"""Connect to net, send binary messages over both transports, and print replies."""

import argparse
import asyncio
import json
import math
import sys

from aiortc import RTCConfiguration, RTCPeerConnection, RTCSessionDescription
from aiortc.sdp import candidate_from_sdp
from websockets.asyncio.client import connect


class Client:
    def __init__(self):
        self.peer = RTCPeerConnection(RTCConfiguration(iceServers=[]))
        self.socket = None
        self.channel = None
        self.ready = asyncio.Event()
        self.failure = None

        @self.peer.on("datachannel")
        def on_datachannel(channel):
            self.channel = channel

            def opened():
                if self.ready.is_set():
                    return
                if channel.label != "game" or channel.ordered or channel.maxRetransmits != 0:
                    self.failure = "Expected 'game' channel, unordered with maxRetransmits=0"
                else:
                    print("Connected (unordered WebRTC, zero retransmissions)", flush=True)
                self.ready.set()

            channel.on("open", opened)

            @channel.on("message")
            def on_message(data):
                self.received("unreliable", data)

            @channel.on("close")
            async def on_close():
                if self.socket is not None:
                    await self.socket.close()

            # aiortc may deliver a remote channel after it has already opened.
            if channel.readyState == "open":
                opened()

        @self.peer.on("connectionstatechange")
        async def on_state():
            if self.peer.connectionState == "failed":
                self.failure = "WebRTC connection failed"
                if self.socket is not None:
                    await self.socket.close()

    @staticmethod
    def received(transport, data):
        print(f"Received {transport}: {data!r}", flush=True)

    async def send_reliable(self, data: bytes):
        await self.socket.send(data)  # bytes => binary frame, str => signalling

    def send_unreliable(self, data: bytes):
        if self.channel is None or self.channel.readyState != "open":
            raise RuntimeError("Data channel is not open")
        self.channel.send(data)

    async def receive(self):
        # One reader serializes SDP and ICE processing while also handling data.
        async for data in self.socket:
            if isinstance(data, bytes):
                self.received("reliable", data)
                continue
            message = json.loads(data)
            if message.get("sdp"):
                await self.peer.setRemoteDescription(
                    RTCSessionDescription(sdp=message["sdp"], type="offer")
                )
                answer = await self.peer.createAnswer()
                # The pinned Rust stack initiates SCTP only as the DTLS client,
                # while aiortc's ICE-controlled side waits for SCTP initiation.
                # Accept this server's actpass offer as the DTLS server so the
                # Rust peer initiates SCTP, rather than both endpoints waiting.
                answer.sdp = answer.sdp.replace("a=setup:active\r\n", "a=setup:passive\r\n")
                await self.peer.setLocalDescription(answer)
                # aiortc gathers ICE in setLocalDescription; send the completed
                # SDP including candidates, rather than relying on trickle events.
                await self.socket.send(json.dumps({
                    "type": "answer", "sdp": self.peer.localDescription.sdp,
                }))
            elif message.get("candidate") is not None:
                info = message["candidate"]
                value = info["candidate"]
                if not value:
                    await self.peer.addIceCandidate(None)
                    continue
                candidate = candidate_from_sdp(value.removeprefix("candidate:"))
                candidate.sdpMid = info.get("sdpMid")
                candidate.sdpMLineIndex = info.get("sdpMLineIndex")
                await self.peer.addIceCandidate(candidate)
            else:
                raise ValueError("Unrecognized signalling message")

    async def exchange(self, message, duration):
        await asyncio.wait_for(self.ready.wait(), timeout=15)
        if self.failure:
            raise RuntimeError(self.failure)
        data = message.encode("utf-8")
        await self.send_reliable(data)
        print(f"Sent reliable: {data!r}", flush=True)
        self.send_unreliable(data)
        print(f"Sent unreliable: {data!r}", flush=True)
        if duration:
            await asyncio.sleep(duration)
        else:
            await asyncio.Future()  # Receive until disconnected or Ctrl+C.

    async def run(self, url, message, duration=0):
        tasks = []
        try:
            async with connect(url, open_timeout=10, close_timeout=3, proxy=None) as socket:
                self.socket = socket
                print(f"WebSocket connected to {url}; negotiating WebRTC…", flush=True)
                receiver = asyncio.create_task(self.receive())
                sender = asyncio.create_task(self.exchange(message, duration))
                tasks = [receiver, sender]
                try:
                    done, _ = await asyncio.wait(tasks, return_when=asyncio.FIRST_COMPLETED)
                    for task in done:
                        task.result()
                    if self.failure:
                        raise RuntimeError(self.failure)
                    if receiver in done and not self.ready.is_set():
                        raise RuntimeError("Server disconnected before WebRTC opened")
                finally:
                    for task in tasks:
                        task.cancel()
                    await asyncio.gather(*tasks, return_exceptions=True)
        finally:
            await self.peer.close()
            print("Disconnected", flush=True)


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="127.0.0.1", help="Server hostname or IP address")
    parser.add_argument("--port", type=int, default=3000, help="WebSocket port (default: 3000)")
    parser.add_argument("--secure", action="store_true", help="Use wss:// through a TLS endpoint")
    parser.add_argument("--message", default="Hello from Python!", help="Send this text on both transports")
    parser.add_argument("--duration", type=float, default=0,
                        help="Seconds to listen after sending; 0 waits until Ctrl+C")
    args = parser.parse_args()
    args.host = args.host.strip().removeprefix("[").removesuffix("]")
    if not args.host or any(c.isspace() or c in "/?#@" for c in args.host):
        parser.error("--host must be a hostname or IP address, without scheme or path")
    if not 1 <= args.port <= 65535:
        parser.error("--port must be between 1 and 65535")
    if not math.isfinite(args.duration) or args.duration < 0:
        parser.error("--duration must be a finite, nonnegative number")
    return args


async def main(args):
    host = f"[{args.host}]" if ":" in args.host else args.host
    url = f"{'wss' if args.secure else 'ws'}://{host}:{args.port}"
    await Client().run(url, args.message, args.duration)


if __name__ == "__main__":
    args = arguments()
    try:
        asyncio.run(main(args))
    except KeyboardInterrupt:
        pass
    except Exception as error:
        print(f"Error: {str(error) or type(error).__name__}", file=sys.stderr)
        sys.exit(1)
