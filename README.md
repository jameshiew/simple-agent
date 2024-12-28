# simple-agent

Testing out a simple agent that can run shell commands inside a Docker container.

System prompt and example tasks in `system/`:

- `task_build_run.md` - build and run a program in Go
- `task_python_fibonacci.md` - Python script that outputs the first n Fibonacci numbers
- `task_wikipedia_title.md` - download the Wikipedia homepage and parse the title

Task is selected in `docker-compose.yml` and any files written by the agent should appear in the `workspace/` directory.

## Requirements

- ollama
- Docker
- able to run the [qwen2.5-coder:32b-instruct-q5_K_M](https://ollama.com/library/qwen2.5-coder:32b-instruct-q5_K_M) model - it may work with smaller Qwen2.5-Coder instruct models

## Quickstart

```shell
ollama pull qwen2.5-coder:32b-instruct-q5_K_M  # if you use a different model, also update the compose.yml
docker compose build
docker compose up  # to use a different task, update the compose.yml
```
