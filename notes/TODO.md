v_0-25-07-16
1. Get current position!
2. Close current position!
3. New operator/fetcher (...)




v_0-25-07-19:
1. ~~tidy everything up use Decimals in fetchers~~
    2. deprecate the extended market
    3. ~~dont return a (positive response) order if it didnt go through~~
    4. tidy up
    5. ~~add leverage change~~
    6. ~~think about lev~~
2. check if pos is open or not, from fetch
3. if open close position and open position, otherwise just open (first time) -- need to test
4. add cron job tokio
- talk about tokio,
- funding every hour
- how long before does it make sense to do it
- 5 minutes offset
- how long to hold a position?
5. when closing position save it in json

3. ~~add exchange id and native order id~~

1. ~~Worsen price so hft takers dont arb it~~ -- will see


2. tidy up fetchers a lot
trading k razbije na logical orders


3. loop check if order filled, cancel and smaller order, more aggressive
4. ci cd kako bom to prenesu na hetzner, novo verzijo, najbolj scp in cargo run
5. clean up fetchers
6. poglej clean up trade-a, post trade analysis, 
7. aja poglej na koliko časa boš dejansko menjal trade? ali boš dal da vsako uro pogleda in potrade-a, a si maker/taker? a boš dau da na 6 ur potrade-a,
a boš dau da na vsako uro pogleda pa trade-a pa pogleda a se splača
8. 50bips might be too far


9. rename from price to px













ok i would now like us to really tidy up the main.rs file i would like to put the functionality in outside functins below main file with descriptive functions

I would like to se if it is possible to destructure the result from those functions, so like 

(hyperliquid_markets, extended_markets) = getAllMarkets(..) etc

i do want to make this as generic as possible, meaning that there are not going to be only 2 fetchers, 2 operators, 2 markets, but a n of markets and we should loop through it i think

right now is only 2 so maybe it is better that we only do separate functions for these two

also no need to calculate amount based on ask price alone we can do, i need to fix the order px


change price to px


we should check if one goes short/open one has bid other has ask, means we have lower amount or higher with which we can open




start balance = start account balance 1 + start account balance 2
current balance = current balance 1 + current balance 2

// file 1
Start: start balance
Current: current balance
Pnl: current balance - start balance
Pnl in %:  (current balance - 1) / start balance


// file 2 (acts like a test)
Start: start balance
Current: current balance
Pnl: positions pnl aggregate + funding payments aggregate - start balance
Pnl in %:  (positions pnl aggregate + funding payments aggregate - 1) / start balance



funding: (this will be to test)


current balance + fees + funding fees - start balance



1. one should reload all positions once closing existing positions and only continue after with new account data
2. should somehow wait and check to not do nothing until a position is filled
3. 






TODO: 
1. first try if it works -> extended doesnt work -> ok
2. migrate extended <- you are here -> ok
3. prettify the infra, base.rs operators, fetchers asse
4. make lighter fetcher/operator
5. 




DOUBLE CHECK FUNDING TIMEFRAMES