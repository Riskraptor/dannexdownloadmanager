"""A DNS server for screenshots on a machine whose hosts file stays as it is: it answers one name
(downloads.example, a reserved name) with 127.0.0.1, where the lab server listens, and says every
other doesn't exist. tools/lab/docs-shots.ps1 -LabDns starts it and points its own DDM's DNS at it
(Options → Connection → DNS: plain DNS, your servers), so the shots' addresses read
http://downloads.example/... as they do from GitHub's runner (docs-shots.yml). It listens over
UDP and TCP on the same port (DDM's plain DNS asks over TCP).

    python tools/lab/lab-dns.py --bind 127.0.0.1:53535 --name downloads.example
"""

import argparse
import socket
import struct
import threading


def answer(query: bytes, name: str) -> bytes | None:
    if len(query) < 12:
        return None
    qid, flags, qd = struct.unpack(">HHH", query[:6])
    if flags & 0x8000 or qd != 1:
        return None
    # The question: its labels, then type and class.
    i, labels = 12, []
    while i < len(query) and query[i]:
        n = query[i]
        labels.append(query[i + 1:i + 1 + n].decode("ascii", "replace").lower())
        i += 1 + n
    end = i + 5
    if end > len(query):
        return None
    qtype, _qclass = struct.unpack(">HH", query[i + 1:end])
    question = query[12:end]
    rd = flags & 0x0100
    asked = ".".join(labels)
    if asked != name:
        # NXDOMAIN: the name isn't ours.
        return struct.pack(">HHHHHH", qid, 0x8083 | rd, 1, 0, 0, 0) + question
    if qtype != 1:
        # The name, but not an IPv4 address asked (AAAA): no answer of that type.
        return struct.pack(">HHHHHH", qid, 0x8080 | rd, 1, 0, 0, 0) + question
    record = struct.pack(">HHHIH", 0xC00C, 1, 1, 60, 4) + socket.inet_aton("127.0.0.1")
    return struct.pack(">HHHHHH", qid, 0x8080 | rd, 1, 1, 0, 0) + question + record


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--bind", default="127.0.0.1:53535")
    ap.add_argument("--name", default="downloads.example")
    ap.add_argument("--log", default="", help="a file each question and answer is written to")
    a = ap.parse_args()
    host, port = a.bind.rsplit(":", 1)
    addr = (host, int(port))
    name = a.name.lower().rstrip(".")
    log = open(a.log, "a", encoding="utf-8") if a.log else None
    lock = threading.Lock()

    def reply(data: bytes, peer) -> bytes | None:
        out = answer(data, name)
        if log:
            with lock:
                log.write(f"{peer} {data.hex()} -> {out.hex() if out else None}\n")
                log.flush()
        return out

    def tcp_client(conn: socket.socket, peer) -> None:
        with conn:
            conn.settimeout(10)
            try:
                while True:
                    head = conn.recv(2)
                    if len(head) < 2:
                        return
                    (n,) = struct.unpack(">H", head)
                    data = b""
                    while len(data) < n:
                        part = conn.recv(n - len(data))
                        if not part:
                            return
                        data += part
                    out = reply(data, peer)
                    if out:
                        conn.sendall(struct.pack(">H", len(out)) + out)
            except OSError:
                return

    def tcp() -> None:
        srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        srv.bind(addr)
        srv.listen(16)
        while True:
            conn, peer = srv.accept()
            threading.Thread(target=tcp_client, args=(conn, peer), daemon=True).start()

    threading.Thread(target=tcp, daemon=True).start()
    udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    udp.bind(addr)
    while True:
        try:
            data, peer = udp.recvfrom(1500)
        except ConnectionResetError:
            # Windows reports an earlier answer's peer gone as an error on the next receive.
            continue
        out = reply(data, peer)
        if out:
            udp.sendto(out, peer)


if __name__ == "__main__":
    main()
