use crate::api::RpmSearchApi;
use crate::config::Config;
use crate::error::{Result, RpmSearchError};
use crate::mcp::protocol::*;
use crate::mcp::tools::get_tools;
use crate::normalize::Package;
use crate::search::SearchFilters;
use serde_json::Value;
use std::cmp::Ordering;
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
            "search_packages" => self.search_packages(&tool_params.arguments)?,
            "get_package_info" => self.get_package_info(&tool_params.arguments)?,
            "list_repositories" => self.list_repositories()?,
            "compare_versions" => self.compare_versions(&tool_params.arguments)?,
            "get_repository_stats" => self.get_repository_stats(&tool_params.arguments)?,
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

        info!(
            "Getting package info: name='{}', arch={:?}, repo={:?}",
            name, arch, repo
        );

        // Search for exact package name
        let filters = SearchFilters {
            name: None,
            arch,
            repo,
            not_requiring: None,
            providing: None,
        };

        let packages = self.api.search(name, filters)?;

        let matching: Vec<&Package> = packages.iter().filter(|p| p.name == name).collect();

        if matching.is_empty() {
            return Ok(format!("Package '{}' not found.", name));
        }

        let mut result = format!("Package information for '{}':\n\n", name);

        for pkg in matching {
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

    fn compare_versions(&self, args: &Value) -> Result<String> {
        let version1 = args["version1"]
            .as_str()
            .ok_or_else(|| RpmSearchError::Config("Missing 'version1' parameter".to_string()))?;

        let version2 = args["version2"]
            .as_str()
            .ok_or_else(|| RpmSearchError::Config("Missing 'version2' parameter".to_string()))?;

        info!("Comparing versions: '{}' vs '{}'", version1, version2);

        // Parse versions
        let pkg1 = self.parse_version_string(version1)?;
        let pkg2 = self.parse_version_string(version2)?;

        let comparison = pkg1.cmp(&pkg2);

        let result = match comparison {
            Ordering::Less => format!("'{}' is OLDER than '{}'", version1, version2),
            Ordering::Equal => format!("'{}' is EQUAL to '{}'", version1, version2),
            Ordering::Greater => format!("'{}' is NEWER than '{}'", version1, version2),
        };

        Ok(result)
    }

    fn parse_version_string(&self, version_str: &str) -> Result<Package> {
        // Parse epoch:version-release or version-release
        let (epoch_str, rest) = if let Some(idx) = version_str.find(':') {
            (&version_str[..idx], &version_str[idx + 1..])
        } else {
            ("0", version_str)
        };

        let epoch = epoch_str.parse::<i64>().map_err(|_| {
            RpmSearchError::Config(format!("Invalid epoch in version: {}", version_str))
        })?;

        let (version, release) = if let Some(idx) = rest.rfind('-') {
            (&rest[..idx], &rest[idx + 1..])
        } else {
            (rest, "1")
        };

        Ok(Package {
            pkg_id: None,
            name: String::new(),
            epoch: Some(epoch),
            version: version.to_string(),
            release: release.to_string(),
            arch: String::new(),
            summary: String::new(),
            description: String::new(),
            repo: String::new(),
            requires: vec![],
            provides: vec![],
        })
    }

    fn get_repository_stats(&self, args: &Value) -> Result<String> {
        let repo = args["repo"]
            .as_str()
            .ok_or_else(|| RpmSearchError::Config("Missing 'repo' parameter".to_string()))?;

        info!("Getting repository stats: repo='{}'", repo);

        let count = self.api.repo_package_count(repo)?;

        let result = format!(
            "Repository: {}\n\
             Total packages: {}\n",
            repo, count
        );

        Ok(result)
    }
}
