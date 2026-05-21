CREATE OR REPLACE FUNCTION update_follow_counts() RETURNS TRIGGER AS
$$
BEGIN
    IF 'TG_OP' = 'INSERT' THEN
        UPDATE users
        SET followers = followers + 1
        WHERE id = NEW.subscription_id;

        UPDATE users
        SET following = following + 1
        WHERE id = NEW.subscriber_id;

    ELSEIF 'TG_OP' = 'DELETE' THEN
        UPDATE users
        SET followers = followers - 1
        WHERE id = OLD.subscription_id;

        UPDATE users
        SET following = following - 1
        WHERE id = OLD.subscriber_id;

    END IF;

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_follow_counts
    AFTER INSERT OR DELETE
    ON user_subscribers
    FOR EACH ROW
EXECUTE FUNCTION update_follow_counts();