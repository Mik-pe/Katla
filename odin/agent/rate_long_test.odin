package agent

import "core:testing"
import "core:time"

@(test)
test_long_minimum_interval_survives_rolling_window_expiry :: proc(t:^testing.T) {
    limiter:Rate_Limiter; rate_limiter_init(&limiter,5*time.Minute,2); defer rate_limiter_destroy(&limiter)
    first,first_wait:=rate_admit(&limiter,0)
    testing.expect(t,first==.Allowed && first_wait==0)
    waited,remaining:=rate_admit(&limiter,time.Minute)
    testing.expect(t,waited==.Wait && remaining==4*time.Minute && len(limiter.timestamps)==0)
    repeated,repeat_wait:=rate_admit(&limiter,2*time.Minute)
    testing.expect(t,repeated==.Wait && repeat_wait==3*time.Minute && len(limiter.timestamps)==0)
    admitted,admission_wait:=rate_admit(&limiter,5*time.Minute)
    testing.expect(t,admitted==.Allowed && admission_wait==0 && len(limiter.timestamps)==1)
    again,again_wait:=rate_admit(&limiter,6*time.Minute)
    testing.expect(t,again==.Wait && again_wait==4*time.Minute && len(limiter.timestamps)==0)
}
