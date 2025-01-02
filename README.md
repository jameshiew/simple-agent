# simple-agent

Give an LLM a regular UNIX environment and a workspace, and let it run shell commands until it completes some task. Runs in a Docker container, edits files in `workspace/` and reads tasks and prompts from `system/`.

Example tasks:

- [system/task_build_run.md](system/task_build_run.md) - build and run a tiny program in Go
- [system/task_python_fibonacci.md](system/task_python_fibonacci.md) - Python script that outputs the first n Fibonacci numbers
- [system/task_wikipedia_title.md](system/task_wikipedia_title.md) - download the Wikipedia homepage and parse the title

Task is passed as an argument to the `simple-agent` binary in `compose.yml`.

## Why?

This is for quickly experimenting with what LLMs are able to achieve with only straightforward shell access rather than well-defined tools specified ahead of time like [ChatGPT functions](https://platform.openai.com/docs/guides/function-calling). In the future, a more general AI should be able to use just a regular command line to achieve a lot of tasks.

## Requirements

- [Ollama](https://ollama.com/)
- Docker
- able to run the [qwen2.5-coder:32b-instruct-q5_K_M](https://ollama.com/library/qwen2.5-coder:32b-instruct-q5_K_M) model - it may work with smaller Qwen2.5-Coder instruct models

## Quickstart

```shell
ollama pull qwen2.5-coder:32b-instruct-q5_K_M  # if you use a different model, also update the compose.yml
docker compose build
docker compose up  # to use a different task, update the compose.yml
```
