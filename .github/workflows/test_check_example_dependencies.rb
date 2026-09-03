require "minitest/autorun"
require "tempfile"
require "tmpdir"

require_relative "check_example_dependencies"

class TestExampleDependencies < Minitest::Test
  def test_discovers_linked_producing_specs_recursively
    Dir.mktmpdir do |examples|
      example = File.join(examples, "example")
      input = File.join(example, "input")
      Dir.mkdir(example)
      Dir.mkdir(input)

      root = File.join(example, "spec.yaml")
      producer = File.join(input, "dm.schema.yaml")
      upstream = File.join(input, "upstream.schema.yaml")
      File.write(root, <<~YAML)
        datasets:
          DM: {path: input/dm.csv, schema: input/dm.schema.yaml}
      YAML
      File.write(producer, <<~YAML)
        datasets:
          UPSTREAM: {path: upstream.csv, schema: upstream.schema.yaml}
      YAML
      File.write(upstream, <<~YAML)
        datasets:
          ROOT: {path: dm.csv, schema: ../spec.yaml}
      YAML

      expected = [root, producer, upstream].map do |path|
        File.realpath(path)
      end.sort
      assert_equal expected, ExampleSpecifications.all_paths(examples)
    end
  end

  def test_implicit_lookup_keys_are_dependencies
    spec = Tempfile.new(["implicit-lookup-key-dependency", ".yaml"])
    spec.write(<<~YAML)
      keys: [STUDYID]
      record_lookups:
        - id: EFFECTIVE
          dataset: TRANSACTIONS
      output:
        columns: [AETERM, STUDYID]
      columns:
        - {name: AETERM, derivation: {source: EFFECTIVE.AETERM}}
        - {name: STUDYID, derivation: {literal: STUDY}}
    YAML
    spec.close

    assert_includes check(spec.path), "AETERM references later column STUDYID"
  ensure
    spec&.unlink
  end

  def test_between_value_is_a_lookup_dependency
    spec = Tempfile.new(["between-dependency", ".yaml"])
    spec.write(<<~YAML)
      record_lookups:
        - id: WINDOW
          dataset: WINDOWS
          source: STUDYID
          key: STUDYID
          between: {value: ADY, lower: AWLO, upper: AWHI}
      output:
        columns: [STUDYID, AVISIT, ADY]
      columns:
        - {name: STUDYID, derivation: {literal: STUDY}}
        - {name: AVISIT, derivation: {source: WINDOW.AVISIT}}
        - {name: ADY, derivation: {literal: 1}}
    YAML
    spec.close

    assert_includes check(spec.path), "AVISIT references later column ADY"
  ensure
    spec&.unlink
  end

  def test_between_value_is_an_aggregate_dependency
    spec = Tempfile.new(["aggregate-between-dependency", ".yaml"])
    spec.write(<<~YAML)
      output:
        columns: [STUDYID, NADIR, ADT]
      columns:
        - {name: STUDYID, derivation: {literal: STUDY}}
        - name: NADIR
          derivation:
            aggregate:
              expr: "MIN(ADTRPRE.AVAL)"
              group_by: [ADTRPRE.STUDYID]
              between: {value: ADT, lower: ADTRPRE.ADT}
        - {name: ADT, derivation: {literal: "2025-01-01"}}
    YAML
    spec.close

    assert_includes check(spec.path), "NADIR references later column ADT"
  ensure
    spec&.unlink
  end

  def test_date_impute_bound_is_a_dependency
    spec = Tempfile.new(["date-impute-bound-dependency", ".yaml"])
    spec.write(<<~YAML)
      output:
        columns: [ASTDT, TRTSDT]
      columns:
        - name: ASTDT
          derivation:
            date_impute:
              source: ASTDTC
              month: 1
              day: first
              not_before: TRTSDT
        - {name: TRTSDT, derivation: {literal: "2025-01-01"}}
    YAML
    spec.close

    assert_includes check(spec.path), "ASTDT references later column TRTSDT"
  ensure
    spec&.unlink
  end
end
