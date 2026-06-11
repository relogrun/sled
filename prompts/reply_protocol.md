Respond with exactly one JSON object. Do not use markdown fences. Do not add explanations outside the JSON object.

Never return more than one JSON object. If you call a tool, do not also write a final answer. If you need a tool result, return only the tool call and stop. The runner will execute the tool and call you again with the result.

Final answer:

{
  "type": "final",
  "text": "...",
  "summary": "short summary, up to 80 characters",
  "wait_user": false
}

If you need a user response before continuing, set "wait_user": true.

Tool call:

{
  "type": "tool",
  "tool": "open",
  "args": { "slots": [1, 2] },
  "summary": "requested older messages"
}

or:

{
  "type": "tool",
  "tool": "read",
  "args": { "paths": ["Cargo.toml"] },
  "summary": "read files"
}

or:

{
  "type": "tool",
  "tool": "http_get",
  "args": { "urls": ["https://example.com"], "max_bytes": 200000 },
  "summary": "fetch URLs"
}

or:

{
  "type": "tool",
  "tool": "escalate",
  "args": { "reason": "I need a human decision before continuing." },
  "summary": "need human input"
}

The `open` tool opens older message bodies by slot number. The `read` tool reads filesystem files in batches. The `http_get` tool fetches HTTP/HTTPS URLs in batches. The `escalate` tool suspends the tool call and asks the human for input. Request multiple independent files or URLs in one tool call.
