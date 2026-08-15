# Portfolio

## JobHunting {#jobhunting}

Automated job search and CV tailoring platform — this project.
- **Pipeline**: scrape JobStreet listings → generate tailored CVs via LLM
- **Stack**: Rust + Axum + HTMX + SQLite + Docker
- **AI**: OpenRouter API (DeepSeek, Claude) with streaming SSE responses
- **Frontend**: Server-rendered HTML, zero JavaScript framework, progressive enhancement
- **Scraping**: Playwright with session cookie management and anti-detection

[[index#experience|Back to experience]]

## Proxmox & Virtualization {#proxmox}

Dual-node hyper-converged homelab cluster running 12+ VMs and LXC containers
with VLAN-segmented networking and NFS-backed storage.

- **Hardware**: pve-ryzen (AMD Ryzen 9 5900X, 12C/24T, 128GB RAM) + pve-router
  (Intel N150, 4C/4T, 7GB RAM) — running Proxmox VE 9.x (Kernel 6.17)
- **VMs**: TrueNAS (NAS/storage backbone), Docker hosts (internal + public-facing),
  Nomad cluster worker, Pomerium identity proxy, PocketID auth, Netbird mesh
  VPN, Pangolin reverse tunnel, Manis AI workload
- **Containers (LXC)**: Yuri mesh node, Tailscale routers, AI workloads
- **Storage**: local-lvm for VM root disks, TrueNAS NFS pools (20T+) for
  bulk storage, multi-pool backup strategy
- **Networking**: VLAN-aware bridges (7 VLANs), PVE API automation via
  curl/Python, QEMU Guest Agent for headless VM management
- **Cluster features**: Proxmox API with ticket-based auth, live VM/CT lifecycle,
  backup/restore across NFS shares

[[index#experience|Back to experience]]

## OpenWrt & Router Engineering {#openwrt}

Multi-WAN, segmented routing infrastructure running as virtualized appliances
on Proxmox — designed for reliability, security, and network isolation.

- **Main router** (VM 9999, OpenWrt 25.12-rc2): Primary PPPoE WAN (XL Axiata),
  backup WAN DHCP (separate physical port), TR-069 management VLAN. Manages
  dnsmasq DHCP/DNS across all VLANs, firewall zones with nftables, and policy
  routing between WAN paths.
- **DMZ router** (VM 9998, OpenWrt 25.12-rc3): Security boundary hosting
  WireGuard wg0, Tailscale mesh (100.x.x.x/8), Cloudflare WARP tunnel
  (IPv4/IPv6 egress via table 1111), and UDP wire tunnel. Dual routing tables
  for WAN vs. WARP egress.
- **VLAN segmentation**: 7 VLANs — native/management (99), trusted (20),
  IoT (30), guest (40), remote (50), DMZ (100), WAN/PPPoE (1111) — each with
  dedicated firewall zone policies (ACCEPT/REJECT/DROP forwarding rules).
- **Multi-WAN**: Primary PPPoE fiber (metric 0), backup ISP DHCP (metric 20),
  TR-069 management (metric 100) — independent routing tables with failover.
- **VPN mesh**: Tailscale (10+ nodes), WireGuard point-to-point, Cloudflare
  WARP global tunnel, custom UDP wire tunnel to external services.
- **Custom firmware**: Compiled ImmortalWrt from source for GL-iNet routers.
  Built and deployed AmneziaWG kernel modules for OpenWrt.
- **Physical**: GPON fiber (Huawei ONT) → OpenWrt → 2.5GbE backbone to
  Proxmox hosts and Gigabit switching for client devices.

[[index#experience|Back to experience]]

## Self-Hosting & Homelab Infrastructure {#selfhosting}

Production-adjacent self-hosted service stack running on Proxmox with
observability, auth, and automated backup.

- **NAS**: TrueNAS VM with 20T+ NFS storage backing all Proxmox VM/CT images,
  container registries, backups, and bulk data storage — provisioned across
  multiple export paths.
- **Docker hosts**: Segregated by exposure level — internal host (Pomerium,
  PocketID, Netbird, Nomad worker) and public-facing host (Pangolin tunnel,
  Manis AI). Both run Docker Compose with health-check-based automatic restarts.
- **Observability**: Prometheus metric collection with custom alert rules,
  Grafana dashboards for system and service monitoring, structured JSON logging
  with health-check endpoints.
- **Reverse proxy & auth**: Caddy with auto-HTTPS, Pomerium identity-aware
  proxy, PocketID authentication service, Pangolin tunnel for public endpoints.
- **Mesh VPN**: Tailscale mesh covering 10+ nodes across VMs, containers,
  laptops, and mobile devices — secure inter-node communication without
  exposed ports.
- **Infrastructure**: PiKVM on Armbian for out-of-band KVM access, UPS with
  NUT monitoring, systemd-networkd for host-level networking, automated
  configuration backups for routers and VMs.
- **Monitoring**: Regular network quality analysis (latency, jitter, packet
  loss), service health verification, and backup integrity checks.

[[index#experience|Back to experience]]

## Indonesian Survey Map {#survey-map}

Interactive choropleth map for Indonesian judicial survey data (Indeks Kinerja
Peradilan), built for Komisi Yudisial and used internally to export survey
statistics and map visuals into reports. Displays province-level survey scores
across multiple respondent categories with year-over-year trends (2022-2025).
- **Stack**: Vite + Vanilla JS + Leaflet + Chart.js + Cloudflare Pages
- **Architecture**: Static JSON API (pre-generated), zero database runtime overhead
- **Performance**: Lighthouse score >95, time to interactive <2s, $0/month hosting
- **Migration**: SQLite to static JSON API (597KB bundle size reduction)
- **Deployed**: Cloudflare Pages free tier

[[index#experience|Back to experience]]

## Indeks Integritas Hakim {#indeks-hakim}

Judge integrity survey platform for Komisi Yudisial (Judicial Commission of
Indonesia). Sole developer maintaining and improving a legacy CodeIgniter 3
PHP application used by ~50 surveyors interviewing ~2,000 judges per year.
- **Migration**: Broken undocumented app → on-prem Ubuntu 22.04 + Apache2 + MariaDB
- **Security**: Fixed SQL injection vulnerability in login form
- **Documentation**: Created comprehensive docs from scratch (install guides, user manuals)
- **Tooling**: Python charts generator, survey data exporter, question generator — cut report generation from days/weeks to seconds/minutes
- **Features**: Survey validation, browser storage persistence, GeoJSON map visualizer

[[index#experience|Back to experience]]

## youtube-watch-history-to-csv {#yt-history}

Python tool that converts YouTube watch history from Google Takeout to CSV
format for bulk scrobbling on Last.fm via universalscrobbler.com.
- **Stars**: 45 on GitHub, 5 forks
- **Stack**: Python, HTML parsing, CSV export
- **License**: MIT
- **Impact**: Solves a real problem for Last.fm users with large YouTube histories

[[index#experience|Back to experience]]

## CAD & 3D Printing {#cad-3d-printing}

Hobbyist mechanical designer since 2017. Started with a heavily modified Ender 3 v1
and now iterate on a Bambu Lab X1C. Workflows in Fusion 360 and Onshape, optimized
for FDM printing, assembly tolerance, and practical homelab use.

- **[Fanatec QR1/QR2 wall mount](https://cad.onshape.com/documents/a1bdb11e09e48f5c19c82f88/w/1c7a3ead739e8e33a05569bd/e/a73e8c37ad3bf740704e4d2d?renderMode=0&uiState=6a4a1b783c1bece0705025f4)** — wall mount optimized for easy FDM printing and strength.
- **[Treadmill desk mount for Kettler Track S6](https://cad.onshape.com/documents/548bccd7ad21129f12b7296c/w/93dccd13cfadf3a5613b5f23/e/a3765aefb9733c6eef423763?renderMode=0&uiState=6a4a1d3041f6b105150eb3e8)** — modular, non-invasive desk mount so you can walk/run while working; uses common, easy-to-clean parts.
- **[Zero-support HDD caddy for generic server rack](https://cad.onshape.com/documents/1acb76356ccfc170ac87852b/w/dadff49574f8236944bb0f39/e/cd58ce9ef5c6c4d62783891d?renderMode=0&uiState=6a4a1ef931edf2f9d595c2bb)** — server-rack HDD caddy with zero supports and forgiving tolerances.
- **[4U → 5U generic server case expander](https://cad.onshape.com/documents/f126867c6b290fc190278026/w/fd6b083fa9d5f11240eb85f4/e/bd910159a98fd451a45eece0?renderMode=0&uiState=6a4a1f9c31edf2f9d595c56d)** — case expander for standard 19" server racks.

[[index#experience|Back to experience]]
