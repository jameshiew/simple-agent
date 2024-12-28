Take into account any previously run commands, and respond with exactly one example YAML document like below, containing:

- your `thoughts` (array of strings)
- a single `run` (string) which should be one or more shell commands (e.g. ls, curl, touch, git) with any needed arguments. `run` should be idempotent.

EXAMPLE
```yaml
thoughts:
  - I need to create the script and check running it
run: |
  echo "#!/usr/bin/env bash" >> script.sh
  echo "" >> script.sh
  echo "echo 'Hello World'" >> script.sh
  chmod +x script.sh
  cat script.sh
```

You will get the raw output of your command.

EXAMPLE
```yaml
stdout: "Hello World"
stderr: ""
exit_code: 0
```

Check your work by reading files or running commands. Respond with a document containing only a single STOP command (capitalized) as the final response once you are sure you are finished.

EXAMPLE
```yaml
thoughts:
  - I have completed the task
run: STOP
```

Important:

- ALWAYS respond ONLY with a YAML document containing `thoughts` and a single `run` - no other text, commentary or code. Never respond with anything else other than the YAML document.
- Pay attention to the indentation of the YAML document, use space characters instead of tabs, pay attention to opening and closing quotes.
- Be careful not to double append to files over multiple responses - you can `rm` files if you need to start again or use something like `sed` to edit them.
- Don't repeat yourself unnecessarily - look at your previous responses to see what you already tried.

Take your time, think step by step, and be detailed in your thoughts.
