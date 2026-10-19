/*
  Class that implements a client for our rust server. 
  Crucially, it implements sendUnreliable() and the necessary signalling in the constructor. 
*/
class Client {
  constructor(url, onReady, onMessage, onClose, onWebSocketOpen = () => {}) {
    this.closed = false;
    this.ready = false;
    this.peer = new RTCPeerConnection({ iceServers: [] });
    this.socket = new WebSocket(url);
    this.socket.binaryType = "arraybuffer";
    this.onClose = onClose;
    this.timer = setTimeout(() => this.close("WebRTC connection timed out"), 15000);
    this.socket.onopen = () => {
      if (!this.closed) onWebSocketOpen();
    };

    // WebRTC needs a separate signalling connection to exchange setup details.
    // Here the WebSocket carries both JSON signalling (text frames) and reliable
    // application data (binary frames). WebRTC establishes its own UDP transport
    // for the unreliable data channel; those messages do not go over WebSocket.
    // No STUN/TURN servers are configured, so we rely on reachable host candidates.

    // The Rust server starts negotiation by sending an SDP offer, followed by
    // ICE candidates. WebSocket preserves message order, but it does not wait
    // for an async message handler to finish. This promise chain processes each
    // signalling message completely before the next, so addIceCandidate cannot
    // run before setRemoteDescription has applied the offer.
    let signalling = Promise.resolve();
    this.socket.onmessage = ({ data }) => {
      // Binary frames are application messages, so they bypass SDP/ICE parsing.
      if (typeof data !== "string") {
        onMessage("reliable", new Uint8Array(data));
        return;
      }
      signalling = signalling.then(async () => {
        if (this.closed) return;
        const message = JSON.parse(data);
        if (message.sdp) {
          // SDP describes the proposed session: data-channel transport, ICE
          // credentials and the certificate fingerprint used to authenticate
          // the encrypted connection. Applying the server's offer tells the
          // browser what it is answering; this alone does not connect the peers.
          await this.peer.setRemoteDescription({ type: "offer", sdp: message.sdp });
          const answer = await this.peer.createAnswer();
          // Commit our answer locally and start gathering local ICE candidates.
          // Send the answer back so the server can apply its remote description.
          // Candidates discovered afterwards are sent separately below.
          await this.peer.setLocalDescription(answer);
          this.signal({ type: "answer", sdp: answer.sdp });
        } else if (message.candidate) {
          // A candidate describes a possible network endpoint for the server.
          // Adding it lets ICE test candidate pairs and select a working path;
          // it is not an application message or a guarantee of connectivity.
          await this.peer.addIceCandidate(message.candidate);
        } else {
          throw new Error("Unrecognized signalling message");
        }
      }).catch((error) => this.close(error.message));
    };
    this.socket.onerror = () => this.close("WebSocket connection failed");
    this.socket.onclose = () => this.close("WebSocket closed");
    // Send our own candidates as the browser discovers them ("trickle ICE").
    // The server adds these to its peer connection, just as we add its candidates.
    // A null candidate marks gathering completion and is omitted by this protocol.
    this.peer.onicecandidate = ({ candidate }) => {
      if (candidate && !this.closed) {
        this.signal({ type: "ice", candidate: candidate.toJSON() });
      }
    };
    this.peer.onconnectionstatechange = () => {
      if (["failed", "closed"].includes(this.peer.connectionState)) {
        this.close(`WebRTC ${this.peer.connectionState}`);
      }
    };
    // The server creates the "game" channel. The browser receives it through
    // WebRTC's data-channel negotiation, rather than creating a second channel
    // here. SDP/ICE establishes the transport; this event exposes the channel.
    this.peer.ondatachannel = ({ channel }) => {
      if (channel.label !== "game" || this.channel) {
        this.close("Unexpected data channel");
        return;
      }
      this.channel = channel;
      channel.binaryType = "arraybuffer";
      channel.onmessage = ({ data }) => {
        const bytes = typeof data === "string"
          ? new TextEncoder().encode(data) : new Uint8Array(data);
        onMessage("unreliable", bytes);
      };
      channel.onclose = () => this.close("Data channel closed");
      channel.onerror = () => this.close("Data channel failed");
      const opened = () => {
        if (this.closed || this.ready) return;
        // Wait for the channel to open before notifying the application. An open
        // WebSocket or a completed SDP exchange alone is not sufficient. The
        // server's channel settings apply in both directions: unordered delivery
        // and zero retransmissions allow messages to be lost or arrive out of order.
        if (channel.ordered || channel.maxRetransmits !== 0) {
          this.close("Expected an unordered data channel with maxRetransmits=0");
          return;
        }
        this.ready = true;
        clearTimeout(this.timer);
        onReady();
      };
      channel.onopen = opened;
      if (channel.readyState === "open") opened();
    };
  }

  signal(message) {
    if (this.socket.readyState === WebSocket.OPEN) {
      this.socket.send(JSON.stringify(message));
    }
  }

  sendReliable(bytes) {
    if (!this.ready || this.closed) throw new Error("Client is not connected");
    this.socket.send(bytes);
  }

  sendUnreliable(bytes) {
    if (!this.ready || this.closed) throw new Error("Client is not connected");
    this.channel.send(bytes);
  }

  close(reason = "Disconnected") {
    if (this.closed) return;
    this.closed = true;
    clearTimeout(this.timer);
    this.peer.close();
    this.socket.close();
    this.onClose(reason);
  }
}

/*
  Demo plumbing code.
*/
const element = (id) => document.getElementById(id);
const query = new URLSearchParams(location.search);
for (const key of ["host", "port", "scheme", "message"]) {
  if (query.has(key)) element(key).value = query.get(key);
}
let client;
const log = (message) => {
  // Bound the log for long-running sessions; textContent never interprets HTML.
  element("log").textContent = (element("log").textContent + message + "\n").slice(-20000);
};
function controls(connecting, ready = false) {
  element("connect").disabled = connecting;
  element("disconnect").disabled = !connecting;
  element("reliable").disabled = !ready;
  element("unreliable").disabled = !ready;
}
function send(transport) {
  try {
    const bytes = new TextEncoder().encode(element("message").value);
    if (transport === "reliable") client.sendReliable(bytes);
    else client.sendUnreliable(bytes);
    log(`Sent ${transport}: ${JSON.stringify(element("message").value)}`);
  } catch (error) {
    client?.close(error.message);
  }
}
element("connect-form").onsubmit = (event) => {
  event.preventDefault();
  client?.close();
  try {
    const host = element("host").value.trim().replace(/^\[|\]$/g, "");
    if (!host || /[\s/?#@]/.test(host)) throw new Error("Enter a hostname or IP address");
    const port = Number(element("port").value);
    if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error("Invalid port");
    const scheme = element("scheme").value;
    if (!["ws", "wss"].includes(scheme)) throw new Error("Invalid WebSocket scheme");
    const url = `${scheme}://${host.includes(":") ? `[${host}]` : host}:${port}`;
    controls(true);
    element("status").textContent = `Connecting to ${url}…`;
    client = new Client(url, () => {
      controls(true, true);
      element("status").textContent = "Connected (unordered WebRTC, zero retransmissions)";
      log("WebRTC data channel opened (unordered, zero retransmissions)");
    }, (transport, bytes) => {
      log(`Received ${transport}: ${JSON.stringify(new TextDecoder().decode(bytes))} [${bytes.length} bytes; hex ${Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("")}]`);
    }, (reason) => {
      controls(false);
      element("status").textContent = reason;
      log(reason);
    }, () => {
      log("WebSocket opened");
    });
  } catch (error) {
    controls(false);
    element("status").textContent = error.message;
  }
};
element("disconnect").onclick = () => client?.close();
element("reliable").onclick = () => send("reliable");
element("unreliable").onclick = () => send("unreliable");
window.addEventListener("pagehide", () => client?.close());
if (query.has("host") || query.has("port")) element("connect-form").requestSubmit();
