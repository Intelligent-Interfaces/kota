#!/usr/bin/env python3
import sys
import json
import urllib.request
import urllib.parse

def send_response(req_id, result=None, error=None):
    resp = {
        "jsonrpc": "2.0",
        "id": req_id
    }
    if error:
        resp["error"] = error
    else:
        resp["result"] = result
    sys.stdout.write(json.dumps(resp) + "\n")
    sys.stdout.flush()

def search_boston_data(query):
    try:
        url = f"https://data.boston.gov/api/3/action/package_search?q={urllib.parse.quote(query)}"
        req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
        with urllib.request.urlopen(req, timeout=10) as response:
            data = json.loads(response.read().decode("utf-8"))
            results = data.get("result", {}).get("results", [])
            
            lines = []
            for pkg in results[:5]:
                title = pkg.get("title", "No Title")
                notes = pkg.get("notes", "No description available")
                resources = pkg.get("resources", [])
                
                lines.append(f"📦 DATASET: {title}")
                lines.append(f"   Description: {notes[:200]}...")
                lines.append("   Resources/Files:")
                for r in resources[:3]:
                    r_name = r.get("name", "Unnamed resource")
                    r_id = r.get("id", "N/A")
                    r_fmt = r.get("format", "unknown")
                    lines.append(f"     - {r_name} (Format: {r_fmt}, Resource ID: {r_id})")
                lines.append("")
            
            output = "\n".join(lines) if lines else "No datasets found in Boston open data for this query."
            return {"content": [{"type": "text", "text": output}]}
    except Exception as e:
        return {"content": [{"type": "text", "text": f"Error searching Boston Open Data: {str(e)}"}]}

def get_boston_resource(resource_id):
    try:
        url = f"https://data.boston.gov/api/3/action/datastore_search?resource_id={urllib.parse.quote(resource_id)}&limit=10"
        req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
        with urllib.request.urlopen(req, timeout=10) as response:
            data = json.loads(response.read().decode("utf-8"))
            records = data.get("result", {}).get("records", [])
            
            if not records:
                return {"content": [{"type": "text", "text": "No records found in this resource datastore."}]}
            
            lines = [f"📊 Records for resource {resource_id} (showing first 10):"]
            for idx, r in enumerate(records):
                lines.append(f"\n[{idx + 1}]")
                for k, v in r.items():
                    if k != "_id":
                        lines.append(f"   {k}: {v}")
            
            return {"content": [{"type": "text", "text": "\n".join(lines)}]}
    except Exception as e:
        return {"content": [{"type": "text", "text": f"Error reading Boston resource: {str(e)}"}]}

def search_socrata_data(domain, query):
    try:
        # Socrata discovery API for Cambridge / Somerville
        url = f"https://api.us.socrata.com/api/catalog/v1?q={urllib.parse.quote(query)}&domains={domain}&limit=5"
        req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
        with urllib.request.urlopen(req, timeout=10) as response:
            data = json.loads(response.read().decode("utf-8"))
            results = data.get("results", [])
            
            lines = []
            for item in results:
                resource = item.get("resource", {})
                name = resource.get("name", "Unnamed dataset")
                desc = resource.get("description", "No description available.")
                item_id = resource.get("id", "N/A")
                permalink = resource.get("permalink", "")
                
                lines.append(f"📦 DATASET: {name}")
                lines.append(f"   ID/Resource ID: {item_id}")
                lines.append(f"   Description: {desc[:200]}...")
                if permalink:
                    lines.append(f"   Link: {permalink}")
                lines.append("")
                
            output = "\n".join(lines) if lines else f"No datasets found in {domain} for this query."
            return {"content": [{"type": "text", "text": output}]}
    except Exception as e:
        return {"content": [{"type": "text", "text": f"Error searching {domain}: {str(e)}"}]}

def main():
    while True:
        try:
            line = sys.stdin.readline()
            if not line:
                break
            
            req = json.loads(line.strip())
            req_id = req.get("id")
            method = req.get("method")
            params = req.get("params", {})
            
            if method == "initialize":
                result = {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "boston-mcp-server",
                        "version": "1.0.0"
                    }
                }
                send_response(req_id, result)
                
            elif method == "notifications/initialized":
                # client acknowledges initialization, nothing to return
                pass
                
            elif method == "tools/list":
                tools = [
                    {
                        "name": "search_boston_data",
                        "description": "Search the City of Boston open data portal (data.boston.gov) for datasets (e.g. food establishments, permits, budget, 311).",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "query": {
                                    "type": "string",
                                    "description": "Search keyword or dataset topic."
                                }
                            },
                            "required": ["query"]
                        }
                    },
                    {
                        "name": "get_boston_resource",
                        "description": "Fetch records from a specific Boston Datastore resource (using a Resource ID obtained from searching).",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "resource_id": {
                                    "type": "string",
                                    "description": "The unique Resource ID of the file/datastore."
                                }
                            },
                            "required": ["resource_id"]
                        }
                    },
                    {
                        "name": "search_cambridge_data",
                        "description": "Search the City of Cambridge open data portal (data.cambridgema.gov) for municipal datasets and resources.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "query": {
                                    "type": "string",
                                    "description": "Search keyword or topic."
                                }
                            },
                            "required": ["query"]
                        }
                    },
                    {
                        "name": "search_somerville_data",
                        "description": "Search the City of Somerville open data portal (data.somervillema.gov) for municipal datasets and resources.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "query": {
                                    "type": "string",
                                    "description": "Search keyword or topic."
                                }
                            },
                            "required": ["query"]
                        }
                    }
                ]
                send_response(req_id, {"tools": tools})
                
            elif method == "tools/call":
                tool_name = params.get("name")
                arguments = params.get("arguments", {})
                
                if tool_name == "search_boston_data":
                    res = search_boston_data(arguments.get("query", ""))
                    send_response(req_id, res)
                elif tool_name == "get_boston_resource":
                    res = get_boston_resource(arguments.get("resource_id", ""))
                    send_response(req_id, res)
                elif tool_name == "search_cambridge_data":
                    res = search_socrata_data("data.cambridgema.gov", arguments.get("query", ""))
                    send_response(req_id, res)
                elif tool_name == "search_somerville_data":
                    res = search_socrata_data("data.somervillema.gov", arguments.get("query", ""))
                    send_response(req_id, res)
                else:
                    send_response(req_id, error={"code": -32601, "message": f"Tool not found: {tool_name}"})
            else:
                if req_id is not None:
                    send_response(req_id, error={"code": -32601, "message": f"Method not found: {method}"})
        except Exception as e:
            # Prevent server crash on bad line input, respond with error
            pass

if __name__ == "__main__":
    main()
