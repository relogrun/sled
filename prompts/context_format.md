The context contains:

1. The system prompt.
2. An index of all dialog files: slot number, role, status, summary. Empty open slots use role `none`.
3. The message bodies currently opened in context.

If the index shows a message you need but its body is not included in the opened bodies, use `open`.
