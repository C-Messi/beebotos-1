---
name: beebotos-startup
description: Use when the user wants to start, restart, stop, or check BeeBotOS local services in this repo, especially gateway on 8000 and web on 8090.
---

# BeeBotOS Startup

Use only in `/Users/qsh/Documents/work/beebotos`.

## Services

- Gateway: port `8000`
- Web: port `8090`
- BeeHub: port `8080`

## Workflow A: Start

First build with the project script:

```bash
./beebotos-dev.sh run gateway
./beebotos-dev.sh run web
```

Use `run` after code changes because it builds before starting. If the script says `started` but ports are not listening, treat it as failed and use the tmux fallback below.

## Stable macOS Fallback

The script can print `started` while the process dies after the shell exits. On this machine, prefer tmux for long-lived local dev services:

```bash
cp target/release/beebotos-gateway data/run/bee-gw-bin
chmod +x data/run/bee-gw-bin

tmux kill-session -t beebotos-gateway 2>/dev/null || true
tmux kill-session -t beebotos-web 2>/dev/null || true

tmux new-session -d -s beebotos-gateway 'cd /Users/qsh/Documents/work/beebotos && data/run/start-gateway-launchd.sh'
sleep 5
lsof -tiTCP:8000 -sTCP:LISTEN | head -1 > data/run/gateway.pid

tmux new-session -d -s beebotos-web 'cd /Users/qsh/Documents/work/beebotos && target/release/web-server --static-path data/run/web-static --gateway-url http://localhost:8000'
sleep 3
lsof -tiTCP:8090 -sTCP:LISTEN | head -1 > data/run/web.pid
```

`data/run/start-gateway-launchd.sh` should load `.env`, map Kimi variables, and execute the renamed binary:

```bash
source .env
export BEE_MODELS__DEFAULT_PROVIDER="${BEE_MODELS__DEFAULT_PROVIDER:-kimi}"
export KIMI_API_KEY="${KIMI_API_KEY:-${BEE__MODELS__KIMI__API_KEY:-}}"
export ZHIPU_API_KEY="${ZHIPU_API_KEY:-${BEE__MODELS__ZHIPU__API_KEY:-}}"
export DEEPSEEK_API_KEY="${DEEPSEEK_API_KEY:-dev-placeholder}"
exec /Users/qsh/Documents/work/beebotos/data/run/bee-gw-bin
```

If gateway fails with `migration ... was previously applied but is missing`, preserve the old dev DB and let the current repo recreate it:

```bash
ts=$(date +%Y%m%d-%H%M%S)
backup_dir="data/run/db-backups/$ts"
mkdir -p "$backup_dir"
for f in data/beebotos.db data/beebotos.db-wal data/beebotos.db-shm; do
  [ -e "$f" ] && mv "$f" "$backup_dir/"
done
```

Start success criteria:

- Gateway listens on `8000`.
- Web listens on `8090`.
- `curl -i --max-time 3 http://127.0.0.1:8000/health` returns `200`.
- `curl -I --max-time 3 http://127.0.0.1:8090/` returns `200`.

## Workflow B: Stop

Stop both script-managed and tmux-managed services:

```bash
./beebotos-dev.sh stop gateway
./beebotos-dev.sh stop web
tmux kill-session -t beebotos-gateway 2>/dev/null || true
tmux kill-session -t beebotos-web 2>/dev/null || true
rm -f data/run/gateway.pid data/run/web.pid
```

Verify stopped state:

```bash
./beebotos-dev.sh status
lsof -nP -iTCP:8000 -sTCP:LISTEN
lsof -nP -iTCP:8090 -sTCP:LISTEN
curl -i --max-time 3 http://127.0.0.1:8000/health
curl -I --max-time 3 http://127.0.0.1:8090/
```

Stop success criteria:

- No listener on `8000`.
- No listener on `8090`.
- HTTP checks cannot connect.
- `./beebotos-dev.sh status` reports gateway and web stopped.

## Workflow C: Restart

Restart means stop first, then start again:

```bash
./beebotos-dev.sh stop gateway
./beebotos-dev.sh stop web
tmux kill-session -t beebotos-gateway 2>/dev/null || true
tmux kill-session -t beebotos-web 2>/dev/null || true
rm -f data/run/gateway.pid data/run/web.pid

./beebotos-dev.sh run gateway
./beebotos-dev.sh run web
```

If the script start is not live after verification, use the tmux fallback from Workflow A.

Restart success criteria are the same as start success criteria.

## Workflow D: Status

Do not trust script status alone. Verify with status, ports, and HTTP:

```bash
./beebotos-dev.sh status
lsof -nP -iTCP:8000 -sTCP:LISTEN
lsof -nP -iTCP:8090 -sTCP:LISTEN
curl -i --max-time 3 http://127.0.0.1:8000/health
curl -I --max-time 3 http://127.0.0.1:8090/
tmux ls | rg 'beebotos-(gateway|web)'
```

Status success criteria:

- `gateway` is running on `8000`.
- `web` is running on `8090`.
- Web responds from `http://localhost:8090`.
- Final answer reports status in Chinese.

## Logs

```bash
tail -f data/run/gateway.log
tail -f data/run/web.log
```

If the script reports success but ports or HTTP fail, inspect logs and prefer real liveness evidence over PID files.
