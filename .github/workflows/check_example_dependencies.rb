require "csv"
require "set"
require "yaml"

IDENTIFIER = /[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*/
SQL_WORDS = Set.new(%w[AND BETWEEN FALSE IN IS LIKE NOT NULL OR TRUE UNKNOWN])

def values(value)
  value.is_a?(Array) ? value : [value]
end

def variable_dependencies(value, declared, lookup_sources, resolving = Set.new)
  values(value).each_with_object(Set.new) do |item, dependencies|
    item = item["variable"] if item.is_a?(Hash)
    next unless item.is_a?(String)

    qualifier, separator = item.partition(".")
    if separator.empty?
      dependencies << item if declared.include?(item)
      next
    end

    next unless lookup_sources.key?(qualifier)
    next if resolving.include?(qualifier)

    dependencies.merge(
      variable_dependencies(
        lookup_sources.fetch(qualifier),
        declared,
        lookup_sources,
        resolving | [qualifier]
      )
    )
  end
end

def identifier_tokens(text)
  return Set.new unless text.is_a?(String)

  unquoted = text.gsub(/'(?:''|[^'])*'/, " ")
  unquoted.to_enum(:scan, IDENTIFIER).each_with_object(Set.new) do |_match, tokens|
    token = Regexp.last_match[0]
    following = unquoted[Regexp.last_match.end(0)..].to_s.lstrip
    next if !token.include?(".") && following.start_with?("(")

    tokens << token unless SQL_WORDS.include?(token.upcase)
  end
end

def identifier_dependencies(text, declared, lookup_sources)
  identifier_tokens(text).each_with_object(Set.new) do |token, dependencies|

    dependencies.merge(
      variable_dependencies(token, declared, lookup_sources)
    )
  end
end

def template_dependencies(template, declared, lookup_sources)
  return Set.new unless template.is_a?(String)

  dependencies = Set.new
  index = 0
  while index < template.length
    if ["{{", "}}"].include?(template[index, 2])
      index += 2
    elsif template[index] == "{"
      closing = template.index("}", index + 1)
      break unless closing

      dependencies.merge(
        variable_dependencies(
          template[(index + 1)...closing], declared, lookup_sources
        )
      )
      index = closing + 1
    else
      index += 1
    end
  end
  dependencies
end

def ordering_dependencies(order_by, declared, lookup_sources)
  variable_dependencies(order_by, declared, lookup_sources)
end

def expression_dependencies(expression, declared, lookup_sources)
  return Set.new unless expression.is_a?(Hash) && expression.length == 1

  keyword, payload = expression.first
  dependencies = Set.new
  case keyword
  when "source"
    variable = payload.is_a?(Hash) ? payload["variable"] : payload
    dependencies.merge(
      variable_dependencies(variable, declared, lookup_sources)
    )
  when "literal"
    nil
  when "coalesce", "greatest", "least"
    dependencies.merge(
      variable_dependencies(payload["sources"], declared, lookup_sources)
    )
  when "case"
    values(payload["branches"]).each do |branch|
      next unless branch.is_a?(Hash)

      dependencies.merge(
        identifier_dependencies(branch["when"], declared, lookup_sources)
      )
      dependencies.merge(
        expression_dependencies(branch["then"], declared, lookup_sources)
      )
    end
    dependencies.merge(
      expression_dependencies(payload["otherwise"], declared, lookup_sources)
    )
  when "mapping", "mapping_from", "cut", "date_impute", "date_precision",
       "str_extract", "str_upper", "str_lower"
    dependencies.merge(
      variable_dependencies(payload["source"], declared, lookup_sources)
    )
  when "date_diff"
    dependencies.merge(
      variable_dependencies(
        [payload["start"], payload["end"]], declared, lookup_sources
      )
    )
  when "study_day"
    dependencies.merge(
      variable_dependencies(
        [payload["date"], payload["reference"]], declared, lookup_sources
      )
    )
  when "str_concat"
    values(payload["sources"]).each do |source|
      dependencies.merge(
        expression_dependencies(source, declared, lookup_sources)
      )
    end
  when "str_template"
    template = payload.is_a?(Hash) ? payload["template"] : payload
    dependencies.merge(
      template_dependencies(template, declared, lookup_sources)
    )
  when "row_number", "rank"
    dependencies.merge(
      variable_dependencies(payload["group_by"], declared, lookup_sources)
    )
    dependencies.merge(
      ordering_dependencies(payload["order_by"], declared, lookup_sources)
    )
    dependencies.merge(
      identifier_dependencies(payload["filter"], declared, lookup_sources)
    )
  when "row_value"
    dependencies.merge(
      variable_dependencies(
        [payload["source"], *values(payload["group_by"])],
        declared,
        lookup_sources
      )
    )
    dependencies.merge(
      ordering_dependencies(payload["order_by"], declared, lookup_sources)
    )
  when "baseline_flag"
    dependencies.merge(
      variable_dependencies(
        [
          *values(payload["group_by"]),
          payload["date"],
          payload["reference_date"]
        ],
        declared,
        lookup_sources
      )
    )
  when "baseline_value"
    dependencies.merge(
      variable_dependencies(
        [
          *values(payload["group_by"]),
          payload["value"],
          payload["flag"]
        ],
        declared,
        lookup_sources
      )
    )
  when "aggregate"
    aggregate = payload.is_a?(Hash) ? payload : {"expr" => payload}
    dependencies.merge(
      identifier_dependencies(aggregate["filter"], declared, lookup_sources)
    )
    dependencies.merge(
      variable_dependencies(aggregate["group_by"], declared, lookup_sources)
    )
    dependencies.merge(
      variable_dependencies(
        aggregate.dig("between", "value"), declared, lookup_sources
      )
    )
    dependencies.merge(
      identifier_dependencies(aggregate["expr"], declared, lookup_sources)
    )
  when "compute"
    dependencies.merge(
      identifier_dependencies(payload["expr"], declared, lookup_sources)
    )
  when "function"
    (payload["args"] || {}).each_value do |argument|
      if argument.is_a?(String)
        dependencies.merge(
          variable_dependencies(argument, declared, lookup_sources)
        )
      elsif argument.is_a?(Hash)
        dependencies.merge(
          expression_dependencies(argument, declared, lookup_sources)
        )
      end
    end
  else
    raise "unsupported expression keyword #{keyword.inspect}"
  end
  dependencies
end

def derivation_dependencies(derivation, declared, lookup_sources)
  return Set.new unless derivation.is_a?(Hash)

  if derivation.key?("value")
    dependencies = expression_dependencies(
      derivation["value"], declared, lookup_sources
    )
    values(derivation["override"]).each do |override|
      next unless override.is_a?(Hash)

      dependencies.merge(
        identifier_dependencies(override["when"], declared, lookup_sources)
      )
      dependencies.merge(
        expression_dependencies(override["value"], declared, lookup_sources)
      )
    end
    dependencies
  else
    expression_dependencies(derivation, declared, lookup_sources)
  end
end

def find_cycle(graph)
  visiting = []
  visited = Set.new
  visit = lambda do |node|
    if visiting.include?(node)
      start = visiting.index(node)
      return visiting[start..] + [node]
    end
    return nil if visited.include?(node)

    visiting << node
    graph.fetch(node, Set.new).each do |dependency|
      cycle = visit.call(dependency)
      return cycle if cycle
    end
    visiting.pop
    visited << node
    nil
  end
  graph.each_key do |node|
    cycle = visit.call(node)
    return cycle if cycle
  end
  nil
end

def reachable_nodes(graph, start)
  reached = Set.new
  visit = lambda do |node|
    return if reached.include?(node)

    reached << node
    graph.fetch(node, Set.new).each { |dependency| visit.call(dependency) }
  end
  visit.call(start)
  reached
end

def expected_cycle(spec)
  error_path = File.join(File.dirname(spec), "expected", "error.yaml")
  return nil unless File.exist?(error_path)

  error = YAML.safe_load(File.read(error_path))
  return nil unless error["condition"] == "dependency_cycle"

  error.dig("context", "cycle")
end

def check(spec)
  document = YAML.safe_load(File.read(spec))
  columns = document["columns"]
  return ["columns must be a list"] unless columns.is_a?(Array)

  names = columns.map { |column| column["name"] if column.is_a?(Hash) }.compact
  declared = names.to_set
  positions = names.each_with_index.to_h
  problems = []

  output = document.dig("output", "columns")
  if !output.is_a?(Array)
    problems << "missing output.columns"
  else
    problems << "output.columns contains a duplicate column" if output.uniq != output
    unknown = output.reject { |name| declared.include?(name) }
    unless unknown.empty?
      problems << "output.columns contains unknown column(s): #{unknown.join(', ')}"
    end
    Dir[File.join(File.dirname(spec), "expected", "*.csv")].sort.each do |csv|
      header = CSV.open(csv, "r", &:readline)
      next if header == output

      problems << "output.columns does not match #{File.basename(csv)} header"
    end
  end

  lookup_sources = {}
  values(document["record_lookups"]).each do |lookup|
    next unless lookup.is_a?(Hash) && lookup["id"]

    lookup_sources[lookup["id"]] = [
      *values(lookup["source"] || document["keys"]),
      lookup.dig("between", "value")
    ].compact
  end

  graph = names.to_h { |name| [name, Set.new] }
  columns.each do |column|
    next unless column.is_a?(Hash) && graph.key?(column["name"])

    graph[column["name"]].merge(
      derivation_dependencies(column["derivation"], declared, lookup_sources)
    )
  end
  values(document["rows"]).each do |row|
    next unless row.is_a?(Hash) && row["derivations"].is_a?(Hash)

    row["derivations"].each do |name, derivation|
      next unless graph.key?(name)

      graph[name].merge(
        derivation_dependencies(derivation, declared, lookup_sources)
      )
    end

    next unless row.key?("group_by") && row["filter"].is_a?(String)

    filter_tokens = identifier_tokens(row["filter"])
    qualified = filter_tokens.select { |token| token.include?(".") }
    unless qualified.empty?
      problems << "grouped row #{row["id"]} filter contains qualified " \
                  "identifier(s): #{qualified.sort.join(', ')}"
    end

    row_columns = row["derivations"].keys.to_set
    missing = filter_tokens.reject { |token| token.include?(".") }.to_set -
              row_columns
    unless missing.empty?
      problems << "grouped row #{row["id"]} filter references column(s) " \
                  "not derived by that row: #{missing.sort.join(', ')}"
    end
  end

  expected = expected_cycle(spec)
  allowed_edges = Set.new
  if expected.is_a?(Array) && expected.length > 1
    expected_edges = expected.each_cons(2).to_set
    missing_edges = expected_edges.reject do |name, dependency|
      graph.fetch(name, Set.new).include?(dependency)
    end
    unless missing_edges.empty?
      problems << "expected dependency cycle is not present"
    end

    start = expected.first
    forward = reachable_nodes(graph, start)
    component = graph.each_key.select do |node|
      forward.include?(node) && reachable_nodes(graph, node).include?(start)
    end.to_set
    graph.each do |name, dependencies|
      dependencies.each do |dependency|
        allowed_edges << [name, dependency] if component.include?(name) &&
                                                component.include?(dependency)
      end
    end
  end

  graph.each do |name, dependencies|
    dependencies.sort.each do |dependency|
      next if allowed_edges.include?([name, dependency])
      next unless positions.fetch(dependency) > positions.fetch(name)

      problems << "#{name} references later column #{dependency}"
    end
  end

  remaining_graph = graph.transform_values(&:dup)
  allowed_edges.each do |name, dependency|
    remaining_graph.fetch(name, Set.new).delete(dependency)
  end
  cycle = find_cycle(remaining_graph)
  problems << "dependency cycle: #{cycle.join(' -> ')}" if cycle
  problems
rescue Psych::SyntaxError, CSV::MalformedCSVError => error
  ["parse error: #{error.message.lines.first.strip}"]
rescue StandardError => error
  [error.message]
end

if __FILE__ == $PROGRAM_NAME
  examples = File.expand_path("../../yaml/examples", __dir__)
  specs = if ARGV.empty?
            Dir[File.join(examples, "*", "spec*.yaml")].select do |spec|
              File.basename(spec).match?(/\Aspec(?:_[a-z][a-z0-9_]*)?\.yaml\z/)
            end.sort
          else
            ARGV.map { |path| File.expand_path(path) }
          end
  errors = specs.flat_map do |spec|
    relative = spec.delete_prefix("#{Dir.pwd}/")
    check(spec).map { |problem| "#{relative}: #{problem}" }
  end

  if errors.empty?
    puts "Checked dependency and output order in #{specs.length} example specs."
  else
    warn errors.join("\n")
    exit 1
  end
end
