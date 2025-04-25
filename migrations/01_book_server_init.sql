-- Add migration script here
-- Enable foreign keys
PRAGMA foreign_keys = ON;

CREATE TABLE book (
    id INTEGER PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    authors TEXT NOT NULL,
    description TEXT,
    is_public BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE TABLE chapter (
    book_id INTEGER NOT NULL,
    chapter_number CHAR(20) NOT NULL,
    name TEXT NOT NULL,
    PRIMARY KEY (book_id, chapter_number),
    FOREIGN KEY (book_id) REFERENCES book(id) ON DELETE CASCADE
);

CREATE TABLE student (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    name TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE,
    password TEXT NOT NULL
);

CREATE TABLE manager (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    name TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE,
    password TEXT NOT NULL
);

CREATE TABLE teacher_agent (
    book_id INTEGER NOT NULL,
    student_id INTEGER NOT NULL,
    current_chapter_number CHAR(20) NOT NULL,
    memories TEXT NOT NULL,
    update_time DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    PRIMARY KEY (book_id, student_id),
    FOREIGN KEY (book_id) REFERENCES book(id) ON DELETE CASCADE,
    FOREIGN KEY (student_id) REFERENCES student(id) ON DELETE CASCADE
);

CREATE TABLE history_message (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    student_id INTEGER NOT NULL,
    book_id INTEGER NOT NULL,
    content TEXT NOT NULL,
    update_time DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    FOREIGN KEY (student_id) REFERENCES student(id) ON DELETE CASCADE,
    FOREIGN KEY (book_id) REFERENCES book(id) ON DELETE CASCADE
);

CREATE TABLE chapter_progress (
    student_id INTEGER NOT NULL,
    book_id INTEGER NOT NULL,
    chapter_number CHAR(20) NOT NULL,
    status INTEGER CHECK(
        status BETWEEN 0
        AND 2
    ) NOT NULL,
    objectives TEXT NOT NULL,
    update_time DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    PRIMARY KEY (student_id, book_id, chapter_number),
    FOREIGN KEY (student_id) REFERENCES student(id) ON DELETE CASCADE,
    FOREIGN KEY (book_id) REFERENCES book(id) ON DELETE CASCADE,
    FOREIGN KEY (book_id, chapter_number) REFERENCES chapter(book_id, chapter_number) ON DELETE CASCADE
);

CREATE TABLE agent_setting (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    ai_model TEXT NOT NULL,
    token_budget INTEGER NOT NULL
);

INSERT INTO
    agent_setting (ai_model, token_budget)
VALUES
    ('grok-2-latest', 100000);