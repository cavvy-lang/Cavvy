#!/bin/bash
# build.sh — 构建 libcayrt.a 静态链接库
#
# 用法:
#   bash build.sh              # 构建当前平台的库
#   bash build.sh windows      # 交叉编译 Windows x64
#   bash build.sh linux        # 交叉编译 Linux x64
#
# 依赖: clang 或 gcc, ar

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SRC_DIR="$SCRIPT_DIR/cayrt"
OUTPUT_DIR="$SCRIPT_DIR"

# C 源文件列表
SOURCES=(
    "$SRC_DIR/string_ops.c"
    "$SRC_DIR/type_conv.c"
    "$SRC_DIR/ptr_ops.c"
    "$SRC_DIR/memory.c"
    "$SRC_DIR/array_ops.c"
    "$SRC_DIR/allocator.c"
    "$SRC_DIR/rc_cycle.c"
)

CFLAGS="-O2 -std=c11 -Wall -Wextra -fPIC -ffunction-sections -fdata-sections"

build_for_target() {
    local target="$1"
    local output_name="$2"
    local cc="$3"
    local ar="$4"
    local extra_cflags="$5"

    echo "=== Building $output_name for $target ==="

    # 清理旧的目标文件
    rm -f "$OUTPUT_DIR"/*.o

    # 编译每个源文件
    for src in "${SOURCES[@]}"; do
        local basename="$(basename "$src" .c)"
        local obj="$OUTPUT_DIR/${basename}.o"
        echo "  CC $src -> $obj"
        $cc $CFLAGS $extra_cflags -I"$SRC_DIR" -c "$src" -o "$obj"
    done

    # 打包为静态库
    local lib_path="$OUTPUT_DIR/$output_name"
    echo "  AR $lib_path"
    $ar rcs "$lib_path" "$OUTPUT_DIR"/*.o

    # 清理目标文件
    rm -f "$OUTPUT_DIR"/*.o

    echo "  Done: $lib_path"
    echo ""
}

case "${1:-native}" in
    windows)
        # Windows x64 MinGW 交叉编译
        # 优先使用 clang，其次 x86_64-w64-mingw32-gcc
        if command -v clang &> /dev/null; then
            CC="clang"
            AR="llvm-ar"
            EXTRA="-target x86_64-w64-mingw32"
        elif command -v x86_64-w64-mingw32-gcc &> /dev/null; then
            CC="x86_64-w64-mingw32-gcc"
            AR="x86_64-w64-mingw32-ar"
            EXTRA=""
        else
            echo "ERROR: No Windows cross-compiler found (clang or x86_64-w64-mingw32-gcc)"
            exit 1
        fi
        build_for_target "windows-x64" "libcayrt.a" "$CC" "$AR" "$EXTRA"
        ;;

    linux)
        if command -v clang &> /dev/null; then
            CC="clang"
            AR="llvm-ar"
            EXTRA="-target x86_64-unknown-linux-gnu"
        else
            CC="gcc"
            AR="ar"
            EXTRA=""
        fi
        build_for_target "linux-x64" "libcayrt-linux.a" "$CC" "$AR" "$EXTRA"
        ;;

    native)
        # 构建当前平台
        if command -v clang &> /dev/null; then
            CC="clang"
            AR="llvm-ar"
            EXTRA=""
        else
            CC="gcc"
            AR="ar"
            EXTRA=""
        fi

        case "$(uname -s)" in
            MINGW*|MSYS*|CYGWIN*)
                build_for_target "native-windows" "libcayrt.a" "$CC" "$AR" "$EXTRA"
                ;;
            Linux)
                build_for_target "native-linux" "libcayrt-linux.a" "$CC" "$AR" "$EXTRA"
                ;;
            *)
                echo "Unknown platform: $(uname -s)"
                exit 1
                ;;
        esac
        ;;

    *)
        echo "Usage: $0 [native|windows|linux]"
        exit 1
        ;;
esac
