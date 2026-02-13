# Traffic Light State Machine

A state machine modeling a traffic light.

This example demonstrates state transitions using enums and pattern matching.

```lumen
enum Light
  Red
  Yellow
  Green
end

cell transition(current: Light) -> Light
  match current
    Red -> Green
    Green -> Yellow
    Yellow -> Red
  end
end

cell duration(light: Light) -> Int
  match light
    Red -> 30
    Green -> 25
    Yellow -> 5
  end
end

cell light_name(light: Light) -> String
  match light
    Red -> "RED"
    Yellow -> "YELLOW"
    Green -> "GREEN"
  end
end

cell can_cross(light: Light) -> Bool
  match light
    Green -> true
    Red -> false
    Yellow -> false
  end
end

cell run_cycle(current: Light, steps: Int) -> Light
  if steps <= 0
    return current
  end

  let name = light_name(current)
  let time = duration(current)
  let can_walk = can_cross(current)

  print("  [" + name + "] Duration: " + string(time) + "s | Can cross: " + string(can_walk))

  let next = transition(current)
  return run_cycle(next, steps - 1)
end

enum TrafficState
  Stopped(Int)
  Moving(Int)
  Waiting
end

cell describe_state(state: TrafficState) -> String
  match state
    Stopped -> "Stopped (count: " + string(state.0) + ")"
    Moving -> "Moving (speed: " + string(state.0) + ")"
    Waiting -> "Waiting"
  end
end

cell update_state(state: TrafficState, light: Light) -> TrafficState
  match light
    Red ->
      match state
        Moving -> Stopped(0)
        Stopped -> Stopped(state.0 + 1)
        Waiting -> Stopped(0)
      end
    Green ->
      match state
        Stopped -> Moving(10)
        Moving -> Moving(min(state.0 + 5, 60))
        Waiting -> Moving(10)
      end
    Yellow ->
      match state
        Moving -> Waiting
        Stopped -> Stopped(state.0)
        Waiting -> Waiting
      end
  end
end

cell main() -> Null
  print("=== Traffic Light State Machine ===")
  print("")

  print("Light cycle (5 transitions):")
  let final = run_cycle(Red, 5)
  print("  Final state: " + light_name(final))
  print("")

  print("Traffic flow simulation:")
  let light1 = Red
  let car_state = Moving(30)
  print("  Light: " + light_name(light1) + ", Car: " + describe_state(car_state))

  car_state = update_state(car_state, light1)
  print("  After red light: " + describe_state(car_state))

  let light2 = Green
  car_state = update_state(car_state, light2)
  print("  After green light: " + describe_state(car_state))

  car_state = update_state(car_state, light2)
  print("  Continue on green: " + describe_state(car_state))

  let light3 = Yellow
  car_state = update_state(car_state, light3)
  print("  After yellow light: " + describe_state(car_state))

  return null
end
```
