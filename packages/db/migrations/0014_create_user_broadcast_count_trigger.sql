CREATE OR REPLACE FUNCTION update_broadcasts_count() RETURNS TRIGGER AS
$$
BEGIN
    IF 'TG_OP' = 'INSERT' THEN
        UPDATE users
        SET broadcasts = broadcasts + 1
        WHERE id = NEW.creator_id;

    ELSEIF 'TG_OP' = 'UPDATE' THEN
        IF OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL THEN
            UPDATE users
            SET broadcasts = broadcasts - 1
            WHERE id = OLD.creator_id;
        ELSEIF OLD.deleted_at IS NOT NULL AND NEW.deleted_at IS NULL THEN
            UPDATE users
            SET broadcasts = broadcasts + 1
            WHERE id = NEW.creator_id;
        END IF;


    ELSEIF 'TG_OP' = 'DELETE' THEN
        UPDATE users
        SET broadcasts = broadcasts - 1
        WHERE id = OLD.creator_id;

    END IF;

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_broadcasts_count
    AFTER INSERT OR DELETE
    ON broadcasts
    FOR EACH ROW
EXECUTE FUNCTION update_broadcasts_count();