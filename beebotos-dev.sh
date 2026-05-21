#!/usr/bin/env bash
# BeeBotOS Development Manager (Linux/macOS)
# Usage: ./beebotos-dev.sh [menu|build|start|stop|restart|run|pack|status] [service|all]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="${SCRIPT_DIR}"
PID_DIR="${PROJECT_ROOT}/data/run"
LOG_DIR="${PROJECT_ROOT}/data/logs"
mkdir -p "${PID_DIR}"
mkdir -p "${LOG_DIR}"

cd "${PROJECT_ROOT}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

print_header() {
    echo -e "${CYAN}========================================${NC}"
    echo -e "${CYAN}  BeeBotOS Development Manager${NC}"
    echo -e "${CYAN}========================================${NC}"
    echo ""
}

print_error()   { echo -e "${RED}[ERROR]${NC} $1"; }
print_info()    { echo -e "${BLUE}[INFO]${NC} $1"; }
print_success() { echo -e "${GREEN}[OK]${NC} $1"; }
print_warn()    { echo -e "${YELLOW}[WARN]${NC} $1"; }

# Service definitions
# Format: name|package|build_cmd|binary_path|port|description
SERVICES=(
    "gateway|beebotos-gateway|cargo build --release -p beebotos-gateway|target/release/beebotos-gateway|8000|API Gateway"
    "web|beebotos-web||target/release/web-server|8090|Web Frontend Server"
    "beehub|beebotos-beehub|cargo build --release -p beebotos-beehub|target/release/beehub|8080|BeeHub Service"
    "cli||cargo install --path apps/cli --force||0|CLI Tool (install only)"
)

get_service_field() {
    local svc="$1"
    local idx="$2"
    for entry in "${SERVICES[@]}"; do
        IFS='|' read -r name package build_cmd binary port desc <<< "$entry"
        if [[ "$name" == "$svc" ]]; then
            case $idx in
                1) echo "$package" ;;
                2) echo "$build_cmd" ;;
                3) echo "$binary" ;;
                4) echo "$port" ;;
                5) echo "$desc" ;;
            esac
            return
        fi
    done
}

service_names() {
    local names=()
    for entry in "${SERVICES[@]}"; do
        IFS='|' read -r name _ _ _ _ _ <<< "$entry"
        names+=("$name")
    done
    echo "${names[@]}"
}

is_valid_service() {
    local target="$1"
    for name in $(service_names); do
        [[ "$name" == "$target" ]] && return 0
    done
    return 1
}

get_target_args() {
    local cargo_target="${1:-}"
    if [[ -n "$cargo_target" ]]; then
        echo "--target ${cargo_target}"
    fi
}

get_release_dir() {
    local cargo_target="${1:-}"
    if [[ -n "$cargo_target" ]]; then
        echo "${PROJECT_ROOT}/target/${cargo_target}/release"
    else
        echo "${PROJECT_ROOT}/target/release"
    fi
}

get_binary_path() {
    local binary_name="$1"
    local cargo_target="${2:-}"
    local suffix=""
    if [[ "$cargo_target" == *windows* ]]; then
        suffix=".exe"
    fi
    echo "$(get_release_dir "$cargo_target")/${binary_name}${suffix}"
}

copy_required_file() {
    local source="$1"
    local destination="$2"
    if [[ ! -f "$source" ]]; then
        print_error "Required file not found: $source"
        return 1
    fi
    cp "$source" "$destination"
}

build_service() {
    local svc="$1"
    local cargo_target="${2:-}"
    local package
    local cmd
    local desc
    package=$(get_service_field "$svc" 1)
    cmd=$(get_service_field "$svc" 2)
    desc=$(get_service_field "$svc" 5)

    echo -e "${CYAN}----------------------------------------${NC}"
    echo -e "${CYAN}Building: ${desc} (${svc})${NC}"
    echo -e "${CYAN}----------------------------------------${NC}"
    if [[ -n "$cargo_target" ]]; then
        print_info "Cargo target: $cargo_target"
    fi

    if ! command -v cargo >/dev/null 2>&1; then
        print_error "cargo not found in PATH. Please install Rust: https://rustup.rs"
        return 1
    fi

    if [[ -z "$cmd" && "$svc" != "web" ]]; then
        print_warn "No build command for ${svc}, skipping."
        return 0
    fi

    if [[ "$svc" == "web" ]]; then
        if ! command -v trunk >/dev/null 2>&1; then
            print_error "trunk not found in PATH. Please install it: cargo install trunk"
            return 1
        fi

        pushd "${PROJECT_ROOT}/apps/web" >/dev/null
        local old_no_color="${NO_COLOR-}"
        local had_no_color=0
        if [[ -v NO_COLOR ]]; then
            had_no_color=1
        fi
        if [[ "${NO_COLOR-}" == "1" ]]; then
            export NO_COLOR=true
        fi

        if ! trunk build --release; then
            if [[ "$had_no_color" -eq 1 ]]; then
                export NO_COLOR="$old_no_color"
            else
                unset NO_COLOR
            fi
            popd >/dev/null
            print_error "Build failed: web - trunk build failed"
            return 1
        fi

        if [[ "$had_no_color" -eq 1 ]]; then
            export NO_COLOR="$old_no_color"
        else
            unset NO_COLOR
        fi
        popd >/dev/null

        if cargo build -p beebotos-web --bin web-server --features server --release $(get_target_args "$cargo_target"); then
            print_success "Build completed: ${svc}"
            return 0
        fi

        print_error "Build failed: web - cargo build web-server failed"
        return 1
    fi

    if [[ -n "$package" ]]; then
        if cargo build --release -p "$package" $(get_target_args "$cargo_target"); then
            print_success "Build completed: ${svc}"
            return 0
        fi
        print_error "Build failed: ${svc}"
        return 1
    fi

    if eval "$cmd"; then
        print_success "Build completed: ${svc}"
        return 0
    else
        print_error "Build failed: ${svc}"
        return 1
    fi
}

get_pid_file() {
    echo "${PID_DIR}/${1}.pid"
}

is_running() {
    local svc="$1"
    local pid_file=$(get_pid_file "$svc")
    if [[ -f "$pid_file" ]]; then
        local pid=$(cat "$pid_file")
        if kill -0 "$pid" 2>/dev/null; then
            return 0
        fi
    fi
    return 1
}

start_service() {
    local svc="$1"
    local binary=$(get_service_field "$svc" 3)
    local port=$(get_service_field "$svc" 4)
    local desc=$(get_service_field "$svc" 5)
    local pid_file=$(get_pid_file "$svc")

    if [[ -z "$binary" ]]; then
        print_warn "${svc} is not a daemon service, skipping start."
        return 0
    fi

    if is_running "$svc"; then
        print_warn "${svc} is already running (PID: $(cat "$pid_file"))"
        return 0
    fi

    if [[ ! -f "$binary" ]]; then
        print_error "Binary not found: $binary"
        print_info "Please build ${svc} first."
        return 1
    fi

    echo -e "${CYAN}Starting: ${desc} (${svc})${NC}"
    print_info "Binary: $binary"
    print_info "Port: $port"

    if [[ "$svc" == "web" ]]; then
        # 准备临时静态目录，使用 trunk 生成的 apps/web/dist
        local temp_static_dir="${PROJECT_ROOT}/data/temp-web-static"
        local dist_source="${PROJECT_ROOT}/apps/web/dist"
        if [[ ! -d "$dist_source" ]]; then
            print_error "Web dist directory not found: $dist_source"
            print_info "Please build web first: ./beebotos-dev.sh build web"
            return 1
        fi
        rm -rf "$temp_static_dir"
        mkdir -p "$temp_static_dir"
        cp -r "${dist_source}/." "$temp_static_dir/"
        print_info "Static path: $temp_static_dir"
        print_info "Gateway URL: http://localhost:8000"
        nohup "$binary" --static-path "$temp_static_dir" --gateway-url http://localhost:8000 > "${LOG_DIR}/${svc}.log" 2> "${LOG_DIR}/${svc}.err" &
    else
        nohup "$binary" > "${LOG_DIR}/${svc}.log" 2> "${LOG_DIR}/${svc}.err" &
    fi
    local pid=$!
    echo $pid > "$pid_file"

    sleep 1
    if kill -0 "$pid" 2>/dev/null; then
        print_success "${svc} started (PID: $pid)"
    else
        print_error "${svc} failed to start. Check ${LOG_DIR}/${svc}.log"
        rm -f "$pid_file"
        return 1
    fi
}

stop_service() {
    local svc="$1"
    local pid_file=$(get_pid_file "$svc")

    if ! is_running "$svc"; then
        print_warn "${svc} is not running"
        rm -f "$pid_file"
        return 0
    fi

    local pid=$(cat "$pid_file")
    echo -e "${CYAN}Stopping ${svc} (PID: $pid)...${NC}"

    if kill "$pid" 2>/dev/null; then
        local count=0
        while kill -0 "$pid" 2>/dev/null && [[ $count -lt 10 ]]; do
            sleep 0.5
            count=$((count + 1))
        done
        if kill -0 "$pid" 2>/dev/null; then
            kill -9 "$pid" 2>/dev/null || true
            print_warn "${svc} force stopped"
        else
            print_success "${svc} stopped"
        fi
    fi
    rm -f "$pid_file"
}

restart_service() {
    local svc="$1"
    stop_service "$svc"
    sleep 1
    start_service "$svc"
}

build_and_start() {
    local svc="$1"
    build_service "$svc" && start_service "$svc"
}

pack_release() {
    local target="${1:-all}"

    echo -e "${CYAN}----------------------------------------${NC}"
    echo -e "${CYAN}Packing release for target: ${target}${NC}"
    echo -e "${CYAN}----------------------------------------${NC}"

    local cargo_target="${BEEBOTOS_PACKAGE_TARGET:-}"
    local archive_target="${cargo_target:-$(rustc -vV | awk '/^host:/ { print $2 }')}"
    local out_dir="${PROJECT_ROOT}/dist/beebotos"
    local archive="${PROJECT_ROOT}/dist/beebotos-${archive_target}.tar.gz"

    if [[ -n "$cargo_target" ]]; then
        print_info "Packaging cargo target: $cargo_target"
        if ! rustup target list --installed | grep -Fxq "$cargo_target"; then
            print_error "Rust target is not installed: $cargo_target"
            print_info "Install it with: rustup target add $cargo_target"
            return 1
        fi
    else
        print_info "Packaging native Linux target: $archive_target"
    fi

    local build_list=()
    if [[ "$target" == "all" ]]; then
        build_list=(gateway web beehub)
    else
        build_list=("$target")
    fi

    local svc_name
    for svc_name in "${build_list[@]}"; do
        [[ "$svc_name" == "cli" ]] && continue
        if ! build_service "$svc_name" "$cargo_target"; then
            print_error "Cannot pack because build failed: $svc_name"
            return 1
        fi
    done

    rm -rf "${out_dir}"
    mkdir -p "${out_dir}"

    # Copy binaries and assets
    if [[ "$target" == "all" || "$target" == "gateway" ]]; then
        copy_required_file "$(get_binary_path "beebotos-gateway" "$cargo_target")" "${out_dir}/" || return 1
        cp -r "${PROJECT_ROOT}/migrations_sqlite" "${out_dir}/"
    fi
    if [[ "$target" == "all" || "$target" == "web" ]]; then
        copy_required_file "$(get_binary_path "web-server" "$cargo_target")" "${out_dir}/" || return 1
        local pkg_source="${PROJECT_ROOT}/apps/web/dist"
        if [[ ! -d "$pkg_source" ]]; then
            print_error "Web dist directory not found: $pkg_source"
            print_info "Please build the web service first: ./beebotos-dev.sh build web"
            rm -rf "${out_dir}"
            return 1
        fi
        cp -r "${pkg_source}/." "${out_dir}/"
    fi
    if [[ "$target" == "all" || "$target" == "beehub" ]]; then
        local beehub_path
        beehub_path="$(get_binary_path "beehub" "$cargo_target")"
        if [[ -f "$beehub_path" ]]; then
            cp "$beehub_path" "${out_dir}/"
        else
            print_warn "$(basename "$beehub_path") not found, skipping"
        fi
    fi

    # Copy configs if they exist
    if [[ -d "${PROJECT_ROOT}/config" ]]; then
        cp -r "${PROJECT_ROOT}/config" "${out_dir}/"
        local prod_config="${out_dir}/config/web-server.toml"
        if [[ -f "$prod_config" ]]; then
            sed -i \
                -e 's#path = "apps/web/dist"#path = "."#g' \
                -e 's#path = "apps/web"#path = "."#g' \
                "$prod_config"
        fi
    fi

    if [[ -d "${PROJECT_ROOT}/skills" ]]; then
        cp -r "${PROJECT_ROOT}/skills" "${out_dir}/"
    fi

    # Copy runner script
    cp "${PROJECT_ROOT}/beebotos-run.sh" "${out_dir}/"
    chmod +x "${out_dir}/beebotos-run.sh"

    # Create archive
    tar czf "${archive}" -C "${PROJECT_ROOT}/dist" beebotos

    print_success "Release packed: ${archive}"
    echo "Contents:"
    ls -lah "${out_dir}"
}

show_status() {
    echo -e "${CYAN}Service Status${NC}"
    echo -e "${CYAN}----------------------------------------${NC}"
    printf "%-12s %-10s %-8s %s\n" "Service" "Status" "PID" "Port"
    echo "----------------------------------------"
    for entry in "${SERVICES[@]}"; do
        IFS='|' read -r name _ _ binary port desc <<< "$entry"
        if [[ -z "$binary" ]]; then
            printf "%-12s %-10s %-8s %s\n" "$name" "N/A" "-" "install-only"
            continue
        fi
        local pid_file=$(get_pid_file "$name")
        if is_running "$name"; then
            local pid=$(cat "$pid_file")
            printf "%-12s ${GREEN}%-10s${NC} %-8s %s\n" "$name" "running" "$pid" "$port"
        else
            printf "%-12s ${RED}%-10s${NC} %-8s %s\n" "$name" "stopped" "-" "$port"
        fi
    done
}

show_menu() {
    clear
    print_header
    echo "  1) Build"
    echo "     1.1) Build Gateway"
    echo "     1.2) Build Web"
    echo "     1.3) Build CLI"
    echo "     1.4) Build BeeHub"
    echo "     1.5) Build All"
    echo ""
    echo "  2) Start"
    echo "     2.1) Start Gateway"
    echo "     2.2) Start Web"
    echo "     2.3) Start BeeHub"
    echo "     2.4) Start All"
    echo ""
    echo "  3) Stop"
    echo "     3.1) Stop Gateway"
    echo "     3.2) Stop Web"
    echo "     3.3) Stop BeeHub"
    echo "     3.4) Stop All"
    echo ""
    echo "  4) Restart"
    echo "     4.1) Restart Gateway"
    echo "     4.2) Restart Web"
    echo "     4.3) Restart BeeHub"
    echo "     4.4) Restart All"
    echo ""
    echo "  5) Build & Start"
    echo "     5.1) Build & Start Gateway"
    echo "     5.2) Build & Start Web"
    echo "     5.3) Build & Start BeeHub"
    echo "     5.4) Build & Start All"
    echo ""
    echo "  6) Status"
    echo "  7) Pack Release"
    echo "  0) Exit"
    echo ""
    echo -n "Select option: "
}

handle_menu() {
    while true; do
        show_menu
        read -r choice
        echo ""

        case "$choice" in
            1|1.1) build_service gateway ;;
            1.2)  build_service web ;;
            1.3)  build_service cli ;;
            1.4)  build_service beehub ;;
            1.5)
                for svc in gateway web cli beehub; do
                    build_service "$svc" || true
                done
                ;;
            2|2.1) start_service gateway ;;
            2.2)  start_service web ;;
            2.3)  start_service beehub ;;
            2.4)
                for svc in gateway web beehub; do
                    start_service "$svc" || true
                done
                ;;
            3|3.1) stop_service gateway ;;
            3.2)  stop_service web ;;
            3.3)  stop_service beehub ;;
            3.4)
                for svc in gateway web beehub; do
                    stop_service "$svc"
                done
                ;;
            4|4.1) restart_service gateway ;;
            4.2)  restart_service web ;;
            4.3)  restart_service beehub ;;
            4.4)
                for svc in gateway web beehub; do
                    restart_service "$svc" || true
                done
                ;;
            5|5.1) build_and_start gateway ;;
            5.2)  build_and_start web ;;
            5.3)  build_and_start beehub ;;
            5.4)
                for svc in gateway web beehub; do
                    build_and_start "$svc" || true
                done
                ;;
            6) show_status ;;
            7) pack_release all ;;
            0|q|quit|exit) echo "Goodbye!"; exit 0 ;;
            *) print_warn "Invalid option: $choice" ;;
        esac

        echo ""
        read -p "Press Enter to continue..."
    done
}

handle_cli() {
    local action="$1"
    local target="${2:-all}"

    if [[ -n "$target" ]] && ! is_valid_service "$target" && [[ "$target" != "all" ]]; then
        print_error "Unknown service: $target"
        print_info "Available: $(service_names) all"
        exit 1
    fi

    case "$action" in
        build)
            if [[ "$target" == "all" ]]; then
                for svc in gateway web cli beehub; do
                    build_service "$svc" || true
                done
            else
                build_service "$target"
            fi
            ;;
        start)
            if [[ "$target" == "all" ]]; then
                for svc in gateway web beehub; do
                    start_service "$svc" || true
                done
            else
                start_service "$target"
            fi
            ;;
        stop)
            if [[ "$target" == "all" ]]; then
                for svc in gateway web beehub; do
                    stop_service "$svc"
                done
            else
                stop_service "$target"
            fi
            ;;
        restart)
            if [[ "$target" == "all" ]]; then
                for svc in gateway web beehub; do
                    restart_service "$svc" || true
                done
            else
                restart_service "$target"
            fi
            ;;
        run)
            if [[ "$target" == "all" ]]; then
                for svc in gateway web beehub; do
                    build_and_start "$svc" || true
                done
            else
                build_and_start "$target"
            fi
            ;;
        pack)
            pack_release "$target"
            ;;
        status)
            show_status
            ;;
        *)
            print_error "Unknown action: $action"
            echo "Usage: $0 [menu|build|start|stop|restart|run|pack|status] [service|all]"
            echo ""
            echo "Actions:"
            echo "  build    - Compile a service"
            echo "  start    - Start a service"
            echo "  stop     - Stop a service"
            echo "  restart  - Restart a service"
            echo "  run      - Build and start a service"
            echo "  pack     - Package binaries and assets for deployment"
            echo "  status   - Show service status"
            echo "  menu     - Interactive menu (default)"
            echo ""
            echo "Services: $(service_names) all"
            exit 1
            ;;
    esac
}

main() {
    local action="${1:-menu}"
    if [[ "$action" == "menu" ]]; then
        handle_menu
    else
        handle_cli "$@"
    fi
}

main "$@"
