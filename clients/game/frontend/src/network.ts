import { ServerMessage } from '@binding/ServerMessage.ts';
import { addPlayer, updatePlayer, removePlayer } from './scene';

export let dataChannel: RTCDataChannel;
export let ws: WebSocket;
let peerConnection: RTCPeerConnection;

export function setupNetwork() {
    ws = new WebSocket('ws://127.0.0.1:3000');
    ws.binaryType = 'arraybuffer';

    peerConnection = new RTCPeerConnection();

    // Handle WebSocket messages
    ws.onmessage = async (event) => {

        /// Binary data is an app message
        if (event.data instanceof ArrayBuffer) {
            let message = deserializeServerMessage(event);

            if(message) {
                handleServerMessage(message);
            }

            return;
        }

        /// Text data is a signalling message
        await handleSignallingEvent(event);
    };

    // ICE candidate handling
    peerConnection.onicecandidate = (event) => {
        if (event.candidate) {
            ws.send(JSON.stringify({
                type: 'ice',
                candidate: event.candidate
            }));
        }
    };

    // Data channel handling
    peerConnection.ondatachannel = (event) => {
        dataChannel = event.channel;
        dataChannel.onopen = () => {
            console.log('Data channel opened');
        };
        dataChannel.onmessage = (event) => {
            let message = deserializeServerMessage(event);
            if(message) {
                handleServerMessage(message);
            }
        };
        dataChannel.onclose = () => {
            console.log('Data channel closed');
        };

    };

    // Handle WebSocket errors
    ws.onerror = (error) => {
        console.error('WebSocket error:', error);
    };

    ws.onclose = () => {
        console.log('WebSocket connection closed');
    };

}

// Function to deserialize WebRTC DataChannel message
function deserializeServerMessage(event: MessageEvent): ServerMessage | null {
    try {
        // Ensure message.data is an ArrayBuffer
        if (!(event.data instanceof ArrayBuffer)) {
            console.error("Expected ArrayBuffer, got:", typeof event.data);
            return null;
        }

        // Convert ArrayBuffer to Uint8Array
        const uint8Array = new Uint8Array(event.data);

        // Convert Uint8Array to string (assuming UTF-8 encoding)
        const decoder = new TextDecoder("utf-8");
        const jsonString = decoder.decode(uint8Array);

        // Parse JSON string to object
        const deserialized: ServerMessage = JSON.parse(jsonString);

        return deserialized;
    } catch (error) {
        console.error("Deserialization error:", error);
        return null;
    }
}


function handleServerMessage(message: ServerMessage) {
    if ("Update" in message) {
        for (const [key, state] of Object.entries(message.Update)) {
            if (state) { updatePlayer(key, state); }
        }
    } else if ("PlayerJoined" in message) {
        console.log(`Player ${message.PlayerJoined} joined`);
        addPlayer(message.PlayerJoined.toString());
    } else if ("PlayerLeft" in message) {
        console.log(`Player ${message.PlayerLeft} left`);
        removePlayer(message.PlayerLeft.toString());
    } 
}

async function handleSignallingEvent(event: MessageEvent) {
    const message = JSON.parse(event.data);

    if (message.sdp) {
        let sdp: RTCSessionDescriptionInit = {'sdp': message.sdp, 'type': 'offer'}

        await peerConnection.setRemoteDescription(new RTCSessionDescription(sdp));
        const answer = await peerConnection.createAnswer();
        await peerConnection.setLocalDescription(answer);

        ws.send(JSON.stringify(answer));
    } else if (message.candidate) {
        await peerConnection.addIceCandidate(new RTCIceCandidate(message.candidate));
    }
}