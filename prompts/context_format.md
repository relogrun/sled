The context contains:

1. The system prompt.
2. An index of dialog files included in the current context: slot number, role, status, summary. Empty open slots use role `none`.
3. The message bodies currently opened in context.

If you know the slot number of a message you need but its body is not included in the opened bodies, use `open`.
