### Clearing

1. Add a cron job to register all the supported series from method list_series of registry.
2. Add guards so that the methods are called correctly by the correct users.
3. Support multi-asset collateral/settlement.
4. Support other networks/assets.
5. Check security overall.
6. Cron-job to "refresh" the balance of the users on each asset.

### Registry

1. Should the methods to add be called only by a controller? Or open to everybody? Shall i limit the number of request/s per principal?
