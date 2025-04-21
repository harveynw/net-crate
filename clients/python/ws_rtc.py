#
# Test script to open a ws + webrtc connection and send a message down
#

import json
import asyncio
import websockets
from aiortc import RTCPeerConnection, RTCDataChannel, RTCSessionDescription
from aiortc.rtcicetransport import candidate_from_aioice
from aioice.candidate import Candidate
from pprint import pprint

# WebSocket server URL (replace with your server's address)
WEBSOCKET_URI = "ws://127.0.0.1:3000"

# Global peer connection
pc = RTCPeerConnection()

async def handle_data_channel(channel: RTCDataChannel):
    """Handle events for the received data channel."""
    print(f"Data channel '{channel.label}' created")
    channel.send("Hello from func!".encode())

    @channel.on("open")
    def on_open():
        print(f"Data channel '{channel.label}' is open")
        channel.send("Hello from Python!".encode())

    @channel.on("message")
    def on_message(message):
        print(f"Received message: {message}")

    @channel.on("close")
    def on_close():
        print(f"Data channel '{channel.label}' closed")
    


async def signaling(ws):
    """Handle signaling over WebSocket using raw text with a single connection."""
    async for message in ws:
        print(f"Received signaling message: {message}")
        parsed = json.loads(message)
        pprint(parsed)

        if 'sdp' in parsed and parsed['sdp'] is not None:  # SDP starts with "v=0"
            print("Processing SDP Offer...")
            # Received an SDP offer from the server
            await pc.setRemoteDescription(RTCSessionDescription(sdp=parsed['sdp'], type='offer'))

            # Create and send SDP answer
            answer = await pc.createAnswer()
            await pc.setLocalDescription(answer)
            await ws.send(pc.localDescription.sdp)  # Send raw SDP text
            print("Sent SDP answer")

        elif 'candidate' in parsed and parsed['candidate'] is not None:  # ICE candidate prefix
            print('Processing ICE Candidate...')
            candidate_message = parsed['candidate']

            # Received an ICE candidate from the server
            candidate_parsed = Candidate.from_sdp(candidate_message['candidate'])
            pprint(candidate_parsed)
            candidate = candidate_from_aioice(candidate_parsed)
            candidate.sdpMLineIndex = int(candidate_message['sdpMLineIndex'])
            if candidate_message['sdpMid'] is not None:
                candidate.sdpMid = candidate_message['sdpMid']

            await pc.addIceCandidate(candidate)
            print("Added ICE candidate")

async def run_webrtc(ws):
    """Set up WebRTC peer connection and handle ICE candidates."""
    # Handle incoming data channels (server creates it, client accepts)
    @pc.on("datachannel")
    def on_datachannel(channel):
        asyncio.ensure_future(handle_data_channel(channel))

    # Send ICE candidates to the server via WebSocket
    @pc.on("icecandidate")
    async def on_icecandidate(event):
        if event.candidate:
            await ws.send(event.candidate.candidate)  # Send raw candidate text
            print("Sent ICE candidate")

async def main():
    """Main function to connect to WebSocket and run WebRTC with a single connection."""
    async with websockets.connect(WEBSOCKET_URI) as ws:
        print("Connected to WebSocket server")

        # Run WebRTC setup
        await run_webrtc(ws)

        # Start signaling
        await signaling(ws)

if __name__ == "__main__":
    asyncio.run(main())