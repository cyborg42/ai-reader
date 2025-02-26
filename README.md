# book-server

## Introduction

An AI-assisted teaching app based on mdbook.

## Features

- Supports book creation, deletion, reading, searching, and other functions.
- Supports AI-assisted teaching to help students better understand book content.

## Setup

```bash
touch .env

# sqlite database file path, only need in compile time
echo "DATABASE_URL=sqlite:./database/book.db" >> .env

# need in runtime
echo "OPENAI_KEY=your_openai_key" >> .env
echo "OPENAI_BASE_URL=your_openai_base_url" >> .env
echo "AI_MODEL=model_name" >> .env
```

## Tech Stack

- Backend: Rust (axum, sqlx)
- Frontend: Svelte
- Database: SQLite
- Artificial Intelligence: OpenAI API

## Use Cases

### Book Import

Import books (epub, mdbook.zip), generate book summaries and chapter summaries, and import them into the database.

### Learning

Users open a book, an initial teaching plan is generated and saved to the database, a teacher AI agent is created, and users can learn through dialogue.

The teacher agent can use function calling to:

1. Retrieve book content (including table of contents, summaries, specific chapter content)
2. Get information about the student's learning status (including overall learning plan, overall learning progress, chapter-by-chapter learning progress)
The teacher agent's conversation history is saved to the database in real-time, with context length calculated in real-time. If the limit is exceeded, the earliest conversations and corresponding database entries are deleted.

Every 10 minutes/upon exit/when actively clicking save, a separate AI summarizes the conversation content and uses function calling to:

1. Update chapter learning progress
2. Update overall learning progress
3. Update the learning plan
