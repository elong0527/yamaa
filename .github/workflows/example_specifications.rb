require "set"
require "yaml"

module ExampleSpecifications
  SPEC_NAME = /\Aspec(?:_[a-z][a-z0-9_]*)?\.yaml\z/

  module_function

  def root_paths(examples)
    Dir[File.join(examples, "*", "spec*.yaml")].select do |spec|
      File.basename(spec).match?(SPEC_NAME)
    end.sort
  end

  def linked_paths(spec)
    document = YAML.safe_load(File.read(spec))
    datasets = document.is_a?(Hash) ? document["datasets"] : nil
    return [] unless datasets.is_a?(Hash)

    datasets.each_value.each_with_object([]) do |source, paths|
      next unless source.is_a?(Hash) && source["schema"].is_a?(String)

      paths << File.expand_path(source["schema"], File.dirname(spec))
    end
  rescue Psych::Exception, SystemCallError
    []
  end

  def all_paths(examples)
    pending = root_paths(examples)
    seen = Set.new
    specs = []

    until pending.empty?
      spec = pending.shift
      canonical = File.realpath(spec)
      next if seen.include?(canonical)

      seen << canonical
      specs << canonical
      pending.concat(linked_paths(canonical).select { |path| File.file?(path) })
    end

    specs.sort
  end
end
