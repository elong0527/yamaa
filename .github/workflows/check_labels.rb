require "yaml"

def specifications_with_source_contracts(specs)
  pending = specs.dup
  found = {}
  until pending.empty?
    spec = File.expand_path(pending.shift)
    next if found.key?(spec)

    found[spec] = true
    begin
      document = YAML.safe_load(File.read(spec))
    rescue Psych::SyntaxError, Errno::ENOENT
      next
    end
    next unless document.is_a?(Hash) && document["datasets"].is_a?(Hash)

    document["datasets"].each_value do |source|
      next unless source.is_a?(Hash) && source["schema"].is_a?(String)

      pending << File.expand_path(source["schema"], File.dirname(spec))
    end
  end
  found.keys.sort
end

errors = []
examples = File.expand_path("../../yaml/examples", __dir__)
root_specs = Dir[File.join(examples, "*", "spec*.yaml")].select do |spec|
  File.basename(spec).match?(/\Aspec(?:_[a-z][a-z0-9_]*)?\.yaml\z/)
end.sort
specs = specifications_with_source_contracts(root_specs)

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
  puts "All #{specs.length} example specs give every column a non-empty label."
else
  warn errors.join("\n")
  exit 1
end
