# JobHunting login terminal — KasmVNC Chrome for human login + cookie harvest.
# A real user logs into job boards in the noVNC UI (:6901); session.py reads the
# session out over CDP (:9223). This image is NOT used for scraping. See Architecture §2.4, §9.
FROM kasmweb/chrome:1.16.1

USER root

# KasmVNC's startup polls for network devices that don't exist here — short-circuit it.
RUN sed -i 's/function wait_for_network_devices() {/function wait_for_network_devices() { return 0; #/g' /dockerstartup/vnc_startup.sh

# socat forwards Chrome's CDP (localhost:9222) to 0.0.0.0:9223 so the compose network
# (and host) can reach it. focal→jammy fallback covers the base image's apt sources.
RUN apt-get update --allow-releaseinfo-change || true && \
    apt-get install -y --no-install-recommends socat || \
    (sed -i 's/focal/jammy/g' /etc/apt/sources.list && apt-get update && apt-get install -y --no-install-recommends socat) && \
    rm -rf /var/lib/apt/lists/*

# Turn on Chrome remote debugging (CDP) so session.py can read the login
# session out over the websocket.
RUN sed -i 's|/opt/google/chrome/google-chrome --password-store=basic|/opt/google/chrome/google-chrome --password-store=basic --remote-debugging-port=9222 --remote-allow-origins=*|g' /usr/bin/google-chrome

# Entrypoint: launch the CDP forwarder, then hand off to KasmVNC's normal startup.
COPY entrypoint.sh /dockerstartup/entrypoint.sh
RUN chmod +x /dockerstartup/entrypoint.sh

ENTRYPOINT ["/dockerstartup/entrypoint.sh"]
CMD ["--wait"]

USER 1000

# SSL note: KasmVNC hardcodes `-sslOnly 1` in the Xvnc *binary* (no startup script),
# so it refuses plain HTTP. We override it via the VNCOPTIONS env in docker-compose.yml
# (`-sslOnly 0`). Do NOT try to sed it out of vnc_startup.sh — the flag isn't there.
