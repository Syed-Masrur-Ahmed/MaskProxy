import os

from sqlmodel import Session, SQLModel, create_engine

DATABASE_URL = os.environ["DATABASE_URL"]

engine = create_engine(DATABASE_URL)


def create_db_and_tables() -> None:
    SQLModel.metadata.create_all(engine)
    # Idempotent column adds for fields introduced after create_all has already
    # built the table on existing databases. Postgres 9.6+ supports IF NOT EXISTS.
    with engine.begin() as conn:
        conn.exec_driver_sql(
            "ALTER TABLE privacy_configs "
            "ADD COLUMN IF NOT EXISTS mask_organizations BOOLEAN NOT NULL DEFAULT TRUE"
        )


def get_session():
    with Session(engine) as session:
        yield session
