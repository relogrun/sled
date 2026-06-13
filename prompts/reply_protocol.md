Respond with exactly one JSON object. Do not use markdown fences. Do not add explanations outside the JSON object.

Never return more than one JSON object. If you call a tool, do not also write a final answer. If you need a tool result, return only the tool call and stop. The runner will execute the tool and call you again with the result.

Final answer:

{
  "type": "final",
  "text": "...",
  "summary": "short summary, up to 80 characters",
  "wait_user": false,
  "tool": "",
  "args_json": "{}"
}

If you need a user response before continuing, set "wait_user": true.

Tool call:

{
  "type": "tool",
  "text": "",
  "summary": "requested older messages",
  "wait_user": false,
  "tool": "open",
  "args_json": "{\"slots\":[1,2]}"
}

or:

{
  "type": "tool",
  "text": "",
  "summary": "read files",
  "wait_user": false,
  "tool": "read",
  "args_json": "{\"paths\":[\"Cargo.toml\"]}"
}

or:

{
  "type": "tool",
  "text": "",
  "summary": "fetch URLs",
  "wait_user": false,
  "tool": "http_get",
  "args_json": "{\"urls\":[\"https://example.com\"],\"max_bytes\":200000}"
}

or:

{
  "type": "tool",
  "text": "",
  "summary": "request human input",
  "wait_user": false,
  "tool": "escalate",
  "args_json": "{\"reason\":\"I need a human decision before continuing.\"}"
}

The `open` tool opens older message bodies by slot number. The `read` tool reads filesystem files in batches. The `http_get` tool fetches HTTP/HTTPS URLs in batches. The `escalate` tool suspends the tool call and asks the human for input. Request multiple independent files or URLs in one tool call.

For tool calls, put the tool arguments in `args_json` as a JSON object encoded inside a string. Do not emit an `args` object.
