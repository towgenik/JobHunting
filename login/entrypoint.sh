#!/bin/bash
# Wait for Chrome's CDP to come up, then start the socat forwarder (9223 -> 9222).
(while ! ss -tln | grep -q 127.0.0.1:9222; do sleep 2; done; socat TCP-LISTEN:9223,fork,bind=0.0.0.0,reuseaddr TCP:127.0.0.1:9222) &
# Hand off to KasmVNC's normal startup.
exec /dockerstartup/kasm_default_profile.sh /dockerstartup/vnc_startup.sh /dockerstartup/kasm_startup.sh "$@"
