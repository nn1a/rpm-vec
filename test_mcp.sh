#!/bin/bash

# Test MCP server with tools/list request
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | ./target/release/rpm_repo_search mcp-server
