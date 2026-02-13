# Showcase Main

A runnable multi-file showcase that imports shared types.

```lumen
import models: TaskId, Task, Board, BoardStats
import utils: SectionTitle, RenderOptions, DisplayLine

cell make_sample_board() -> Board
  let tasks = [
    Task(id: "T-100", title: "Define module layout", done: true, points: 3),
    Task(id: "T-101", title: "Wire import statements", done: true, points: 5),
    Task(id: "T-102", title: "Add runtime smoke checks", done: false, points: 3),
    Task(id: "T-103", title: "Refresh showcase README", done: false, points: 2)
  ]
  return Board(name: "Showcase Sprint", tasks: tasks)
end

cell find_task(board: Board, id: TaskId) -> String
  for task in board.tasks
    if task.id == id
      return task.title
    end
  end
  return "missing"
end

cell count_done(tasks: list[Task]) -> Int
  let done = 0
  for task in tasks
    if task.done
      done = done + 1
    end
  end
  return done
end

cell sum_done_points(tasks: list[Task]) -> Int
  let points = 0
  for task in tasks
    if task.done
      points = points + task.points
    end
  end
  return points
end

cell build_stats(board: Board) -> BoardStats
  let done = count_done(board.tasks)
  let total = len(board.tasks)
  return BoardStats(
    total: total,
    done: done,
    open: total - done,
    points_done: sum_done_points(board.tasks)
  )
end

cell headline(title: SectionTitle, stats: BoardStats) -> DisplayLine
  let text = title + " | done " + string(stats.done) + "/" + string(stats.total)
  return DisplayLine(text: text, priority: stats.points_done)
end

cell task_line(task: Task, options: RenderOptions) -> String
  let prefix = options.prefix_open
  if task.done
    prefix = options.prefix_done
  end

  if options.show_points
    return prefix + " " + task.title + " (" + string(task.points) + " pts)"
  end
  return prefix + " " + task.title
end

cell render_board(board: Board, options: RenderOptions) -> String
  let out = ""
  for task in board.tasks
    out = out + task_line(task, options) + "\n"
  end
  return out
end

cell main() -> String
  let board = make_sample_board()
  let stats = build_stats(board)

  let options = RenderOptions(
    show_points: true,
    prefix_done: "[x]",
    prefix_open: "[ ]"
  )

  let summary = headline(board.name, stats)
  let focus = find_task(board, "T-101")
  let lines = render_board(board, options)

  let output = summary.text + "\n"
  let output = output + "completed points: " + string(stats.points_done) + "\n"
  let output = output + "focus: " + focus + "\n\n"
  return output + lines
end
```
