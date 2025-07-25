v_0-25-07-16
1. Get current position!
2. Close current position!
3. New operator/fetcher (...)




v_0-25-07-19:
1. tidy everything up use Decimals in fetchers
    2. deprecate the extended market
    3. dont return a fake order if it didnt go through
    4. tidy up
    5. add leverage change
    6. think about lev
2. check if pos is open or not, from fetch
3. if open close position and open position, otherwise just open (first time)
4. add cron job tokio
5. when closing position save it in json


1. Worsen price so hft takers dont arb it