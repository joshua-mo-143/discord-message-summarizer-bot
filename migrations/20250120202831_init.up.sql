-- Add up migration script here
create table if not exists messages (
    id int generated always as identity primary key,
    data jsonb not null,
    created_at timestamptz default current_timestamp not null
);

create table if not exists summaries (
    id int generated always as identity primary key,
    summary varchar not null,
    date date not null,
    created_at timestamptz default current_timestamp not null
);
