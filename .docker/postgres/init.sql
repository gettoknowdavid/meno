create user meno with password 'password';
create database meno_dev owner postgres;
grant all privileges on database meno_dev to meno;