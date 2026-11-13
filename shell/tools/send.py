#!/usr/bin/env python3
"""send.py SOCKET 'json' ... : write JSON lines to a unix socket (shell or compositor control), print replies."""
import socket, sys, time
s = socket.socket(socket.AF_UNIX); s.connect(sys.argv[1]); s.settimeout(0.5)
for line in sys.argv[2:]:
    s.sendall(line.encode() + b"\n")
    try: sys.stdout.write(s.recv(4096).decode())
    except Exception: pass
time.sleep(0.05)
