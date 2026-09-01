require "yaml"

require_relative "example_specifications"

errors = []
examples = File.expand_path("../../yaml/examples", __dir__)
specs = ExampleSpecifications.all_paths(examples)

specs.each do |spec|
  relative_spec = spec.delete_prefix("#{Dir.pwd}/")

  begin
    document = YAML.safe_load(File.read(spec))
  rescue Psych::SyntaxError => error
    errors << "#{relative_spec}: YAML parse error: #{error.message.lines.first.strip}"
    next
  end

  columns = document.is_a?(Hash) ? document["columns"] : nil
  unless columns.is_a?(Array)
    errors << "#{relative_spec}: columns must be a list"
    next
  end

  columns.each_with_index do |column, index|
    unless column.is_a?(Hash)
      errors << "#{relative_spec}: columns[#{index}] must be a mapping"
      next
    end

    label = column["label"]
    next if label.is_a?(String) && !label.strip.empty?

    name = column["name"] || "<unnamed>"
    errors << "#{relative_spec}: columns[#{index}] #{name} has no non-empty label"
  end
end

if errors.empty?
  puts "All #{specs.length} example and producing specs give every column " \
       "a non-empty label."
else
  warn errors.join("\n")
  exit 1
end
