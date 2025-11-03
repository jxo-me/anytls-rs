#!/bin/bash
# 自动化测试脚本 - 带完整日志收集和分析

set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 配置
TEST_DIR="/tmp/anytls_test_$$"
SERVER_PORT=8443
CLIENT_PORT=1080
SERVER_ADDR="127.0.0.1:${SERVER_PORT}"
PASSWORD="test_password_123"

echo "=========================================="
echo "AnyTLS-RS 自动化测试脚本"
echo "=========================================="
echo ""

# 创建测试目录
mkdir -p "${TEST_DIR}"
cd "${TEST_DIR}"
echo "[1/8] 创建测试目录: ${TEST_DIR}"

# 检查二进制文件
if [ ! -f "../target/release/anytls-server" ] || [ ! -f "../target/release/anytls-client" ]; then
    echo -e "${RED}[错误] 二进制文件不存在，请先编译:${NC}"
    echo "  cd $(dirname $0)"
    echo "  cargo build --release --bins"
    exit 1
fi

# 复制二进制文件
cp ../target/release/anytls-server ./
cp ../target/release/anytls-client ./
echo "[2/8] 二进制文件已复制"

# 启动服务器
echo "[3/8] 启动服务器..."
./anytls-server -l "0.0.0.0:${SERVER_PORT}" -p "${PASSWORD}" > server.log 2>&1 &
SERVER_PID=$!
echo "  服务器PID: ${SERVER_PID}"

# 等待服务器启动
sleep 2

# 检查服务器是否运行
if ! ps -p ${SERVER_PID} > /dev/null; then
    echo -e "${RED}[错误] 服务器启动失败${NC}"
    echo "服务器日志:"
    cat server.log
    exit 1
fi
echo "  服务器运行正常"

# 启动客户端
echo "[4/8] 启动客户端..."
./anytls-client -l "127.0.0.1:${CLIENT_PORT}" -s "${SERVER_ADDR}" -p "${PASSWORD}" > client.log 2>&1 &
CLIENT_PID=$!
echo "  客户端PID: ${CLIENT_PID}"

# 等待客户端启动
sleep 3

# 检查客户端是否运行
if ! ps -p ${CLIENT_PID} > /dev/null; then
    echo -e "${RED}[错误] 客户端启动失败${NC}"
    echo "客户端日志:"
    cat client.log
    cleanup_and_exit 1
fi
echo "  客户端运行正常"

# 等待连接建立
echo "[5/8] 等待连接建立..."
sleep 2

# 测试SOCKS5代理
echo "[6/8] 测试SOCKS5代理..."
TEST_URL="http://httpbin.org/get"
TEST_OUTPUT="test_output.json"

if curl -v --socks5-hostname "127.0.0.1:${CLIENT_PORT}" "${TEST_URL}" > "${TEST_OUTPUT}" 2>&1; then
    echo -e "${GREEN}  ✓ curl测试成功${NC}"
    if grep -q "origin" "${TEST_OUTPUT}"; then
        echo -e "${GREEN}  ✓ 响应内容正确${NC}"
    else
        echo -e "${YELLOW}  ⚠ 响应内容可能不正确${NC}"
        cat "${TEST_OUTPUT}"
    fi
else
    CURL_EXIT_CODE=$?
    echo -e "${RED}  ✗ curl测试失败 (退出码: ${CURL_EXIT_CODE})${NC}"
    echo "curl输出:"
    cat "${TEST_OUTPUT}"
fi

# 等待一段时间以便日志收集
sleep 2

# 停止进程
echo "[7/8] 停止进程..."
kill ${CLIENT_PID} 2>/dev/null || true
kill ${SERVER_PID} 2>/dev/null || true
sleep 1

# 等待进程完全退出
for pid in ${CLIENT_PID} ${SERVER_PID}; do
    if ps -p ${pid} > /dev/null 2>&1; then
        echo "  强制终止进程 ${pid}"
        kill -9 ${pid} 2>/dev/null || true
    fi
done
sleep 1

# 分析日志
echo "[8/8] 分析日志..."
echo ""
echo "=========================================="
echo "测试结果摘要"
echo "=========================================="

# 检查关键日志
echo ""
echo "--- 服务器日志检查 ---"
if grep -q "New connection from" server.log; then
    echo -e "${GREEN}✓ 服务器收到连接${NC}"
else
    echo -e "${RED}✗ 服务器未收到连接${NC}"
fi

if grep -q "Connection established" server.log; then
    echo -e "${GREEN}✓ 连接已建立${NC}"
else
    echo -e "${RED}✗ 连接未建立${NC}"
fi

if grep -q "Sending SYNACK" server.log; then
    echo -e "${GREEN}✓ 服务器发送了SYNACK${NC}"
else
    echo -e "${YELLOW}⚠ 服务器未发送SYNACK（可能版本<2或stream_id<2）${NC}"
fi

if grep -q "Handling stream" server.log; then
    echo -e "${GREEN}✓ 服务器处理了流${NC}"
else
    echo -e "${RED}✗ 服务器未处理流${NC}"
fi

echo ""
echo "--- 客户端日志检查 ---"
if grep -q "TLS handshake successful" client.log; then
    echo -e "${GREEN}✓ TLS握手成功${NC}"
else
    echo -e "${RED}✗ TLS握手失败${NC}"
fi

if grep -q "Authentication sent successfully" client.log; then
    echo -e "${GREEN}✓ 认证成功${NC}"
else
    echo -e "${RED}✗ 认证失败${NC}"
fi

if grep -q "Proxy stream created successfully" client.log; then
    echo -e "${GREEN}✓ 代理流创建成功${NC}"
else
    echo -e "${RED}✗ 代理流创建失败${NC}"
fi

if grep -q "SYNACK received" client.log; then
    echo -e "${GREEN}✓ 客户端收到SYNACK${NC}"
else
    echo -e "${YELLOW}⚠ 客户端未收到SYNACK${NC}"
fi

echo ""
echo "--- 错误日志 ---"
ERROR_COUNT=0
if grep -i "error\|failed\|panic" server.log | grep -v "debug\|trace"; then
    echo -e "${RED}服务器错误:${NC}"
    grep -i "error\|failed\|panic" server.log | grep -v "debug\|trace" | head -10
    ERROR_COUNT=$((ERROR_COUNT + 1))
fi

if grep -i "error\|failed\|panic" client.log | grep -v "debug\|trace"; then
    echo -e "${RED}客户端错误:${NC}"
    grep -i "error\|failed\|panic" client.log | grep -v "debug\|trace" | head -10
    ERROR_COUNT=$((ERROR_COUNT + 1))
fi

if [ ${ERROR_COUNT} -eq 0 ]; then
    echo -e "${GREEN}未发现严重错误${NC}"
fi

echo ""
echo "=========================================="
echo "详细日志文件位置:"
echo "  服务器: ${TEST_DIR}/server.log"
echo "  客户端: ${TEST_DIR}/client.log"
echo "  curl输出: ${TEST_DIR}/${TEST_OUTPUT}"
echo ""
echo "查看完整日志:"
echo "  tail -f ${TEST_DIR}/server.log"
echo "  tail -f ${TEST_DIR}/client.log"
echo "=========================================="

# 清理函数
cleanup_and_exit() {
    local exit_code=${1:-0}
    
    # 停止进程
    kill ${CLIENT_PID} 2>/dev/null || true
    kill ${SERVER_PID} 2>/dev/null || true
    sleep 1
    
    for pid in ${CLIENT_PID} ${SERVER_PID}; do
        if ps -p ${pid} > /dev/null 2>&1; then
            kill -9 ${pid} 2>/dev/null || true
        fi
    done
    
    if [ ${exit_code} -ne 0 ]; then
        echo ""
        echo "测试失败，保留日志目录: ${TEST_DIR}"
        echo "请检查日志文件排查问题"
    else
        echo ""
        read -p "删除测试目录? (y/N): " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            cd ..
            rm -rf "${TEST_DIR}"
            echo "测试目录已删除"
        else
            echo "测试目录保留: ${TEST_DIR}"
        fi
    fi
    
    exit ${exit_code}
}

# 设置退出陷阱
trap 'cleanup_and_exit 1' INT TERM

# 测试完成
if [ -f "${TEST_OUTPUT}" ] && grep -q "origin" "${TEST_OUTPUT}"; then
    echo ""
    echo -e "${GREEN}=========================================="
    echo "测试通过！"
    echo "==========================================${NC}"
    cleanup_and_exit 0
else
    echo ""
    echo -e "${RED}=========================================="
    echo "测试失败！"
    echo "==========================================${NC}"
    cleanup_and_exit 1
fi
