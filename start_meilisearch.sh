#!/bin/bash

echo "Starting Meilisearch server..."
echo

# 检查 meilisearch 是否存在
if ! command -v meilisearch &> /dev/null && [ ! -f "./meilisearch" ]; then
    echo "Error: meilisearch not found"
    echo
    echo "Please install Meilisearch:"
    echo "  curl -L https://install.meilisearch.com | sh"
    echo
    echo "Or download from:"
    echo "  https://github.com/meilisearch/meilisearch/releases"
    echo
    exit 1
fi

echo "Meilisearch will start on http://localhost:7700"
echo "Press Ctrl+C to stop the server"
echo

# 启动 Meilisearch 服务器
if command -v meilisearch &> /dev/null; then
    meilisearch --http-addr 127.0.0.1:7700
else
    ./meilisearch --http-addr 127.0.0.1:7700
fi 