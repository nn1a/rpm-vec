use crate::api::RpmSearchApi;
use crate::config::Config;
use crate::error::{Result, RpmSearchError};
use crate::mcp::protocol::*;
use crate::mcp::tools::get_tools;
use crate::normalize::Package;
use crate::search::SearchFilters;
use crate::storage::FindFilter;
use serde_json::Value;

use std::io::{BufRead, BufReader, Write};
use tracing::{debug, error, info};

pub struct McpServer {
    api: RpmSearchApi,
}

impl McpServer {
    pub fn new(config: Config) -> Result<Self> {
        let api = RpmSearchApi::new(config)?;
        Ok(Self { api })
    }

    /// Run the MCP server (stdio mode)
    pub fn run(&self) -> Result<()> {
        info!("MCP server started (stdio mode)");

        let stdin = std::io::stdin();
        let reader = BufReader::new(stdin.lock());
        let mut stdout = std::io::stdout();

        for line in reader.lines() {
            let line = line.map_err(RpmSearchError::Io)?;

            if line.trim().is_empty() {
                continue;
            }

            debug!("Received: {}", line);

            // Parse the raw JSON to check if it's a notification (no "id" field)
            let raw: Value = serde_json::from_str(&line)
                .map_err(|e| RpmSearchError::Config(format!("Invalid JSON: {}", e)))?;

            let is_notification = raw.get("id").is_none_or(|v| v.is_null());

            if is_notification {
                // JSON-RPC 2.0: Notifications MUST NOT be responded to
                self.handle_notification(&raw);
                continue;
            }

            let response = match self.handle_request(&line) {
                Ok(resp) => resp,
                Err(e) => {
                    error!("Error handling request: {}", e);
                    JsonRpcResponse::error(
                        raw.get("id").cloned(),
                        -32603,
                        format!("Internal error: {}", e),
                    )
                }
            };

            let response_json = serde_json::to_string(&response).map_err(|e| {
                RpmSearchError::Storage(format!("Failed to serialize response: {}", e))
            })?;

            debug!("Sending: {}", response_json);
            writeln!(stdout, "{}", response_json).map_err(RpmSearchError::Io)?;
            stdout.flush().map_err(RpmSearchError::Io)?;
        }

        Ok(())
    }

    /// Handle JSON-RPC notifications (no response expected)
    fn handle_notification(&self, raw: &Value) {
        let method = raw
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown");

        match method {
            "notifications/initialized" => {
                info!("Client initialized successfully");
            }
            "notifications/cancelled" => {
                let request_id = raw
                    .pointer("/params/requestId")
                    .cloned()
                    .unwrap_or(Value::Null);
                debug!("Client cancelled request: {}", request_id);
            }
            _ => {
                debug!("Unhandled notification: {}", method);
            }
        }
    }

    fn handle_request(&self, line: &str) -> Result<JsonRpcResponse> {
        let request: JsonRpcRequest = serde_json::from_str(line)
            .map_err(|e| RpmSearchError::Config(format!("Invalid JSON-RPC request: {}", e)))?;

        let result = match request.method.as_str() {
            "initialize" => {
                let init_result = InitializeResult::new();
                serde_json::to_value(init_result)
                    .map_err(|e| RpmSearchError::Storage(format!("Serialization error: {}", e)))?
            }
            "ping" => {
                // MCP spec: ping returns empty object
                serde_json::json!({})
            }
            "tools/list" => {
                let tools = get_tools();
                let result = ToolsListResult { tools };
                serde_json::to_value(result)
                    .map_err(|e| RpmSearchError::Storage(format!("Serialization error: {}", e)))?
            }
            "tools/call" => self.handle_tool_call(&request.params)?,
            "resources/list" => {
                // Return empty resources list for compatibility
                serde_json::json!({ "resources": [] })
            }
            "resources/templates/list" => {
                // Return empty resource templates list
                serde_json::json!({ "resourceTemplates": [] })
            }
            "prompts/list" => {
                // Return empty prompts list for compatibility
                serde_json::json!({ "prompts": [] })
            }
            _ => {
                return Ok(JsonRpcResponse::error(
                    request.id,
                    -32601,
                    format!("Method not found: {}", request.method),
                ));
            }
        };

        Ok(JsonRpcResponse::success(request.id, result))
    }

    fn handle_tool_call(&self, params: &Option<Value>) -> Result<Value> {
        let params = params
            .as_ref()
            .ok_or_else(|| RpmSearchError::Config("Missing tool call parameters".to_string()))?;

        let tool_params: ToolCallParams = serde_json::from_value(params.clone())
            .map_err(|e| RpmSearchError::Config(format!("Invalid tool call params: {}", e)))?;

        let result_text = match tool_params.name.as_str() {
            "rpm_search" => self.search_packages(&tool_params.arguments)?,
            "rpm_package_info" => self.get_package_info(&tool_params.arguments)?,
            "rpm_repositories" => self.list_repositories()?,
            "rpm_file_search" => self.search_by_file(&tool_params.arguments)?,
            "rpm_find" => self.find_packages(&tool_params.arguments)?,
            _ => {
                return Ok(serde_json::to_value(ToolResult::error(format!(
                    "Unknown tool: {}",
                    tool_params.name
                )))
                .unwrap());
            }
        };

        let tool_result = ToolResult::success(result_text);
        serde_json::to_value(tool_result)
            .map_err(|e| RpmSearchError::Storage(format!("Serialization error: {}", e)))
    }

    fn search_packages(&self, args: &Value) -> Result<String> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| RpmSearchError::Config("Missing 'query' parameter".to_string()))?;

        let arch = args.get("arch").and_then(|v| v.as_str()).map(String::from);
        let repo = args.get("repo").and_then(|v| v.as_str()).map(String::from);
        let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

        info!(
            "Searching packages: query='{}', arch={:?}, repo={:?}, top_k={}",
            query, arch, repo, top_k
        );

        let filters = SearchFilters {
            name: None,
            arch,
            repo,
            not_requiring: None,
            providing: None,
        };

        let mut packages = self.api.search(query, filters)?;

        // Limit results to top_k
        packages.truncate(top_k);

        if packages.is_empty() {
            return Ok("No packages found matching the query.".to_string());
        }

        let mut result = format!("Found {} package(s):\n\n", packages.len());
        for (i, pkg) in packages.iter().enumerate() {
            result.push_str(&format!(
                "{}. {} ({})\n   Version: {}\n   Arch: {}\n   Repo: {}\n   Summary: {}\n\n",
                i + 1,
                pkg.name,
                pkg.pkg_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "N/A".to_string()),
                pkg.full_version(),
                pkg.arch,
                pkg.repo,
                pkg.summary
            ));
        }

        Ok(result)
    }

    fn get_package_info(&self, args: &Value) -> Result<String> {
        let name = args["name"]
            .as_str()
            .ok_or_else(|| RpmSearchError::Config("Missing 'name' parameter".to_string()))?;

        let arch = args.get("arch").and_then(|v| v.as_str()).map(String::from);
        let repo = args.get("repo").and_then(|v| v.as_str()).map(String::from);
        let include_files = args
            .get("include_files")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        info!(
            "Getting package info: name='{}', arch={:?}, repo={:?}",
            name, arch, repo
        );

        // Search for exact package name
        let filters = SearchFilters {
            name: None,
            arch: arch.clone(),
            repo: repo.clone(),
            not_requiring: None,
            providing: None,
        };

        let packages = self.api.search(name, filters)?;

        let matching: Vec<&Package> = packages.iter().filter(|p| p.name == name).collect();

        if matching.is_empty() {
            return Ok(format!("Package '{}' not found.", name));
        }

        let mut result = format!("Package information for '{}':\n\n", name);

        for pkg in &matching {
            result.push_str(&format!(
                "Package: {}\n\
                 Version: {}\n\
                 Architecture: {}\n\
                 Repository: {}\n\
                 Summary: {}\n\
                 Description: {}\n",
                pkg.name,
                pkg.full_version(),
                pkg.arch,
                pkg.repo,
                pkg.summary,
                pkg.description
            ));

            if let Some(ref license) = pkg.license {
                result.push_str(&format!("License: {}\n", license));
            }
            if let Some(ref vcs) = pkg.vcs {
                result.push_str(&format!("VCS: {}\n", vcs));
            }

            if !pkg.requires.is_empty() {
                result.push_str("\nRequires:\n");
                for dep in &pkg.requires {
                    result.push_str(&format!("  - {}\n", dep.name));
                }
            }

            if !pkg.provides.is_empty() {
                result.push_str("\nProvides:\n");
                for prov in &pkg.provides {
                    result.push_str(&format!("  - {}\n", prov.name));
                }
            }

            // Include file list if requested
            if include_files {
                if let Some(_pkg_id) = pkg.pkg_id {
                    let files =
                        self.api
                            .list_package_files(&pkg.name, Some(&pkg.arch), Some(&pkg.repo))?;
                    for (_, file_list) in &files {
                        if !file_list.is_empty() {
                            result.push_str(&format!("\nFiles ({}):\n", file_list.len()));
                            for (path, ft) in file_list {
                                let marker = match ft.as_str() {
                                    "dir" => "d",
                                    "ghost" => "g",
                                    _ => " ",
                                };
                                result.push_str(&format!("  [{}] {}\n", marker, path));
                            }
                        }
                    }
                }
            }

            result.push_str("\n---\n\n");
        }

        Ok(result)
    }

    fn list_repositories(&self) -> Result<String> {
        info!("Listing repositories");

        let repos = self.api.list_repositories()?;

        if repos.is_empty() {
            return Ok("No repositories indexed yet.".to_string());
        }

        let mut result = format!("Indexed repositories ({} total):\n\n", repos.len());
        for (i, (repo, count)) in repos.iter().enumerate() {
            result.push_str(&format!("{}. {}: {} package(s)\n", i + 1, repo, count));
        }

        Ok(result)
    }

    fn search_by_file(&self, args: &Value) -> Result<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| RpmSearchError::Config("Missing 'path' parameter".to_string()))?;

        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

        info!("Searching packages by file: path='{}'", path);

        let mut results = self.api.search_file(path)?;
        results.truncate(limit);

        if results.is_empty() {
            return Ok(format!("No packages found containing file '{}'.", path));
        }

        let mut text = format!(
            "Found {} package(s) containing '{}':\n\n",
            results.len(),
            path
        );
        for (i, (pkg, full_path, file_type)) in results.iter().enumerate() {
            let marker = match file_type.as_str() {
                "dir" => "[d]",
                "ghost" => "[g]",
                _ => "   ",
            };
            text.push_str(&format!(
                "{}. {}-{}.{} ({})\n   {} {}\n",
                i + 1,
                pkg.name,
                pkg.full_version(),
                pkg.arch,
                pkg.repo,
                marker,
                full_path,
            ));
        }

        Ok(text)
    }

    fn find_packages(&self, args: &Value) -> Result<String> {
        let filter = FindFilter {
            name: args.get("name").and_then(|v| v.as_str()).map(String::from),
            summary: args
                .get("summary")
                .and_then(|v| v.as_str())
                .map(String::from),
            description: args
                .get("description")
                .and_then(|v| v.as_str())
                .map(String::from),
            provides: args
                .get("provides")
                .and_then(|v| v.as_str())
                .map(String::from),
            requires: args
                .get("requires")
                .and_then(|v| v.as_str())
                .map(String::from),
            file: args.get("file").and_then(|v| v.as_str()).map(String::from),
            arch: args.get("arch").and_then(|v| v.as_str()).map(String::from),
            repo: args.get("repo").and_then(|v| v.as_str()).map(String::from),
            limit: args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize,
        };

        info!("Finding packages with structured filters");

        let results = self.api.find(&filter)?;

        if results.is_empty() {
            return Ok("No packages found matching the given criteria.".to_string());
        }

        let mut text = format!("Found {} package(s):\n\n", results.len());
        for (i, pkg) in results.iter().enumerate() {
            text.push_str(&format!(
                "{}. {}-{}.{} ({})\n   {}\n",
                i + 1,
                pkg.name,
                pkg.full_version(),
                pkg.arch,
                pkg.repo,
                pkg.summary,
            ));
        }

        Ok(text)
    }
}
