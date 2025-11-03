#!/bin/bash
# Linux版本编译脚本

set -e

echo "=== AnyTLS-RS Linux版本编译脚本 ==="
echo ""

# 检查是否安装了cross
if ! command -v cross &> /dev/null; then
    echo "[0/4] 安装cross工具..."
    cargo install cross --git https://github.com/cross-rs/cross
else
    echo "[0/4] cross工具已安装"
fi

# 检查target是否已安装
if ! rustup target list --installed | grep -q "x86_64-unknown-linux-musl"; then
    echo "[1/4] 安装musl target..."
    rustup target add x86_64-unknown-linux-musl
else
    echo "[1/4] musl target已安装"
fi

# 检查Docker是否运行
echo "[2/4] 检查Docker..."
if ! docker info &> /dev/null; then
    echo "❌ 错误: Docker未运行或未安装"
    echo "   请启动Docker Desktop或安装Docker"
    exit 1
fi
echo "✅ Docker运行正常"

echo ""
echo "[3/4] 开始编译Linux版本（使用cross和musl，静态链接）..."
echo "    这可能需要几分钟时间..."
cross build --release --bins --target x86_64-unknown-linux-musl

echo ""
echo "[4/4] 检查编译结果..."
if [ -f "target/x86_64-unknown-linux-musl/release/anytls-server" ] && \
   [ -f "target/x86_64-unknown-linux-musl/release/anytls-client" ]; then
    echo "✅ 编译成功！"
    echo ""
    echo "二进制文件位置:"
    ls -lh target/x86_64-unknown-linux-musl/release/anytls-* | awk '{print $9, "(" $5 ")"}'
    echo ""
    echo "文件信息:"
    file target/x86_64-unknown-linux-musl/release/anytls-server
    file target/x86_64-unknown-linux-musl/release/anytls-client
    echo ""
    echo "可以使用以下命令复制到Linux服务器:"
    echo "  scp target/x86_64-unknown-linux-musl/release/anytls-server user@host:/path/"
    echo "  scp target/x86_64-unknown-linux-musl/release/anytls-client user@host:/path/"
else
    echo "❌ 编译失败或文件未找到"
    exit 1
fi

