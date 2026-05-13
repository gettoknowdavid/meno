CREATE OR REPLACE FUNCTION update_updated_at_column()
    RETURNS TRIGGER AS
$$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION setup_updated_at_triggers() RETURNS VOID AS
$$
DECLARE
    t TEXT;
BEGIN
    FOR t IN
        SELECT table_name
        FROM information_schema.columns
        WHERE column_name = 'updated_at'
          AND table_schema = 'public'
        LOOP
            EXECUTE format('DROP TRIGGER IF EXISTS trigger_%I_updated_at ON %I', t, t);

            EXECUTE format(
                    'CREATE TRIGGER trigger_%I_updated_at
                     BEFORE UPDATE ON %I
                     FOR EACH ROW
                     EXECUTE FUNCTION update_updated_at_column()',
                    t, t
                    );
        END LOOP;
END;
$$ LANGUAGE plpgsql;