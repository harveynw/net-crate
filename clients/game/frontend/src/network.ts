let dataChannel: RTCDataChannel;

import { ServerMessage } from '@binding/ServerMessage.ts';

function getSendButton(): HTMLButtonElement {
    let button = document.querySelector<HTMLButtonElement>('#counter');
    if (button) {
        return button;
    } else {
        throw new Error("No button");
    }
}

export function setupNetwork() {
    const ws = new WebSocket('ws://127.0.0.1:3000');
    const peerConnection = new RTCPeerConnection();

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

    // Handle WebSocket messages
    ws.onmessage = async (event) => {
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
            getSendButton().disabled = false;
        };
        dataChannel.onmessage = (event) => {
            console.log('Received message:', deserializeServerMessage(event));
        };
        dataChannel.onclose = () => {
            console.log('Data channel closed');
            getSendButton().disabled = true;
        };
    };

    // Button click to send message
    getSendButton().onclick = () => {
        if (dataChannel && dataChannel.readyState === 'open') {
            dataChannel.send('Hello from the client!');
            console.log('Message sent');
        }
    };

    // Handle WebSocket errors
    ws.onerror = (error) => {
        console.error('WebSocket error:', error);
    };

    ws.onclose = () => {
        console.log('WebSocket connection closed');
    };

}