# flow-sensors

iio-sensor-proxy client: orientation with lock, ambient light with classes and auto-brightness curve (hysteresis), proximity, tilt from IIO sysfs. Sensors are claimed only while subscribed.

**Status:** Implemented; the VM has no sensors so only the logic and the absent-hardware path ran.

## TODO(feature)

- PropertiesChanged signals instead of 500 ms polling while subscribed
- IIO buffer/trigger reads for tilt
- Proximity-based screen-off during calls
- Step counter / other IIO channels
