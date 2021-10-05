# Olivia Oracle Server

Olivia makes cryptographic attestations to the outcome of real world events.
It is under documented because it is under development at each level.


## Install

``` sh
git clone https://github.com/LLFourn/olivia.git
cd olivia
cargo install --path .
```

## Run

A lot of config needs to be done before olivia can be useful.
Take a look at `sample_config/ticker.yml` which will just attest to the time and a heads or tails coin toss every minute.


``` sh
olivia --config sample_config/ticker/yml run
```

You can get the REST API at localhost:8000

``` sh
curl localhost:8000 
```

### Running propoerly

The right way to use olivia at the moment is to use postgres as the backend and to use redis to events and outcomes to be attested to.


``` yaml
# olivia_config.yml
database:
  backend: "postgres"
  url: "postgres://postgres@localhost/olivia"

events:
  /NBA/match:
    - type: "redis"
      url: "redis://my-redis-host"
      lists:
        - "NBA:events"

outcomes:
  /NBA/match:
    - type: "redis"
      url: "redis://my-redis-host"
      lists:
        - "NBA:outcomes"
```

Then you must initialize the postgres database:

``` sh
olivia --config=olivia_config.yml db init
```

and then on `my-redis-host` you have can insert the event for [this NBA game](https://www.nba.com/game/mil-vs-bkn-0042000217) like

``` sh
# For this game for example
redis-cli lpush NBA:events '{"id" : "/NBA/match/2021-06-20/MIL_BKN.winner", expected_outcome_time: "2021-06-21T02:00:00"}'
```

and if all goes well it will show up in the HTTP API.

To complete the event you do something similar:

``` sh
# if MIL wins
redis-cli lpush NBA:outcomes '{"id" : "/NBA/match/2021-06-20/MIL_BKN.winner", "outcome": "MIL"}'
```

Exactly what constitutes a valid "outcome" depends on the even type (in this case `winner`).
This isn't documented yet but check the code `olivia_core` to see what the syntax is.
