// Canonical WASP oracle dumper for the ruhit conformance suite.
//
// This tool is intentionally small: it links the pinned WASP HIT/SON/HIVE
// libraries and prints a stable JSON projection of the semantic tree.  The
// projection drops WASP's decorative nodes (brackets, decl, =, quotes, ...)
// but keeps paths, values, array parts, comments, includes, spans and exact
// diagnostics.
//
// Usage:
//   wasp_dump hit <input-file> [--name <stream-name>]
//   wasp_dump hive <schema-file> <input-file>
//
// The process working directory is used for !include resolution, matching
// WASP's own tests.

#include "wasphit/HITInterpreter.h"
#include "wasphit/HITNodeView.h"
#include "waspson/SONInterpreter.h"
#include "waspson/SONNodeView.h"
#include "wasphive/HIVE.h"
#include "waspcore/wasp_node.h"

#include <cctype>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

using namespace wasp;

namespace
{
std::string json_escape(const std::string& text)
{
    std::string out;
    out.reserve(text.size() + 8);
    for (unsigned char c : text)
    {
        switch (c)
        {
        case '"': out += "\\\""; break;
        case '\\': out += "\\\\"; break;
        case '\b': out += "\\b"; break;
        case '\f': out += "\\f"; break;
        case '\n': out += "\\n"; break;
        case '\r': out += "\\r"; break;
        case '\t': out += "\\t"; break;
        default:
            if (c < 0x20)
            {
                static const char* hex = "0123456789abcdef";
                out += "\\u00";
                out += hex[(c >> 4) & 0xf];
                out += hex[c & 0xf];
            }
            else
            {
                out += static_cast<char>(c);
            }
        }
    }
    return out;
}

std::string jstr(const std::string& text)
{
    return "\"" + json_escape(text) + "\"";
}

const char* node_name(std::size_t type)
{
    switch (type)
    {
    case DOCUMENT_ROOT: return "document";
    case DECL: return "decl";
    case TERM: return "term";
    case INTEGER: return "integer";
    case REAL: return "real";
    case KEY: return "key";
    case ASSIGN: return "assign";
    case QUOTE: return "quote";
    case STRING: return "string";
    case QUOTED_STRING: return "quoted-string";
    case VALUE: return "value";
    case COMMENT: return "comment";
    case KEYED_VALUE: return "keyed-value";
    case OBJECT_TERM: return "object-term";
    case OBJECT: return "object";
    case SEMICOLON: return "semicolon";
    case LBRACKET: return "l-bracket";
    case RBRACKET: return "r-bracket";
    case ARRAY: return "array";
    case wasp::FILE: return "file";
    case OVERRIDE_ASSIGN: return "override-assign";
    default: return "other";
    }
}

bool has_child_of_type(HITNodeView node, std::size_t type)
{
    for (std::size_t i = 0; i < node.child_count(); ++i)
    {
        if (node.child_at(i).type() == type)
        {
            return true;
        }
    }
    return false;
}

HITNodeView first_child_of_type(HITNodeView node, std::size_t type)
{
    for (std::size_t i = 0; i < node.child_count(); ++i)
    {
        HITNodeView child = node.child_at(i);
        if (child.type() == type)
        {
            return child;
        }
    }
    return HITNodeView();
}

void emit_span(std::ostream& out, HITNodeView node)
{
    // End positions are deliberately omitted from the semantic projection.
    // WASP reports inclusive last columns while ruhit stores half-open spans;
    // comparing starts keeps the oracle focused on semantics.  ruhit's own
    // span invariants are covered by the unit/corpus tests.
    out << "\"line\":" << node.line() << ",\"column\":" << node.column();
}

void emit_value(std::ostream& out, HITNodeView value)
{
    out << "{\"value\":" << jstr(value.data())
        << ",\"valueType\":" << jstr(node_name(value.token_type())) << "}";
}

void emit_parameter(std::ostream& out, HITNodeView node)
{
    out << "{\"kind\":\"parameter\",\"path\":" << jstr(node.path())
        << ",\"name\":" << jstr(node.name());

    HITNodeView assign = first_child_of_type(node, ASSIGN);
    if (assign.is_null())
    {
        assign = first_child_of_type(node, OVERRIDE_ASSIGN);
    }
    out << ",\"assign\":" << jstr(assign.is_null() ? "" : assign.data())
        << ",\"override\":"
        << (has_child_of_type(node, OVERRIDE_ASSIGN) ? "true" : "false");

    std::vector<HITNodeView> values;
    for (std::size_t i = 0; i < node.child_count(); ++i)
    {
        HITNodeView child = node.child_at(i);
        if (child.type() == VALUE)
        {
            values.push_back(child);
        }
    }
    out << ",\"values\":[";
    for (std::size_t i = 0; i < values.size(); ++i)
    {
        if (i != 0)
        {
            out << ",";
        }
        emit_value(out, values[i]);
    }
    out << "]";
    if (!values.empty())
    {
        out << ",\"value\":" << jstr(values.front().data())
            << ",\"valueType\":" << jstr(node_name(values.front().token_type()));
    }
    out << ",";
    emit_span(out, node);
    out << "}";
}

void emit_array(std::ostream& out, HITNodeView node)
{
    out << "{\"kind\":\"array\",\"path\":" << jstr(node.path())
        << ",\"name\":" << jstr(node.name())
        << ",\"assign\":"
        << jstr(first_child_of_type(node, ASSIGN).is_null()
                    ? (first_child_of_type(node, OVERRIDE_ASSIGN).is_null()
                           ? ""
                           : first_child_of_type(node, OVERRIDE_ASSIGN).data())
                    : first_child_of_type(node, ASSIGN).data())
        << ",\"override\":"
        << (has_child_of_type(node, OVERRIDE_ASSIGN) ? "true" : "false")
        << ",\"parts\":[";

    bool first = true;
    for (std::size_t i = 0; i < node.child_count(); ++i)
    {
        HITNodeView child = node.child_at(i);
        if (child.type() != QUOTE && child.type() != SEMICOLON &&
            child.type() != VALUE)
        {
            continue;
        }
        if (!first)
        {
            out << ",";
        }
        first = false;
        out << "{\"partType\":" << jstr(node_name(child.type()))
            << ",\"value\":" << jstr(child.data());
        if (child.type() == VALUE)
        {
            out << ",\"valueType\":" << jstr(node_name(child.token_type()));
        }
        out << "}";
    }
    out << "],";
    emit_span(out, node);
    out << "}";
}

void emit_block(std::ostream& out, HITNodeView node)
{
    HITNodeView decl = first_child_of_type(node, DECL);
    out << "{\"kind\":\"block\",\"path\":" << jstr(node.path())
        << ",\"name\":" << jstr(node.name())
        << ",\"header\":" << jstr(decl.is_null() ? node.name() : decl.data())
        << ",";
    emit_span(out, node);
    out << "}";
}

void emit_comment(std::ostream& out, HITNodeView node)
{
    out << "{\"kind\":\"comment\",\"path\":" << jstr(node.path())
        << ",\"text\":" << jstr(node.data()) << ",";
    emit_span(out, node);
    out << "}";
}

void emit_include(std::ostream& out, HITNodeView node)
{
    std::string text = node.data();
    std::string target = text;
    const std::string prefix = "!include";
    if (target.compare(0, prefix.size(), prefix) == 0)
    {
        target.erase(0, prefix.size());
        while (!target.empty() && std::isspace(static_cast<unsigned char>(target.front())))
        {
            target.erase(target.begin());
        }
    }
    out << "{\"kind\":\"include\",\"path\":" << jstr(node.path())
        << ",\"name\":" << jstr(node.name()) << ",\"text\":" << jstr(text)
        << ",\"target\":" << jstr(target) << ",";
    emit_span(out, node);
    out << "}";
}

void emit_entry(std::ostream& out, HITNodeView node, bool& first)
{
    bool emit = false;
    switch (node.type())
    {
    case DOCUMENT_ROOT:
    case OBJECT:
    case KEYED_VALUE:
    case ARRAY:
    case COMMENT:
    case wasp::FILE: emit = true; break;
    default: break;
    }
    if (!emit)
    {
        return;
    }
    if (!first)
    {
        out << ",";
    }
    first = false;
    switch (node.type())
    {
    case DOCUMENT_ROOT:
        out << "{\"kind\":\"document\",\"path\":\"/\"}";
        break;
    case OBJECT: emit_block(out, node); break;
    case KEYED_VALUE: emit_parameter(out, node); break;
    case ARRAY: emit_array(out, node); break;
    case COMMENT: emit_comment(out, node); break;
    case wasp::FILE: emit_include(out, node); break;
    default: break;
    }
}

void walk(std::ostream& out, HITNodeView node, bool& first)
{
    emit_entry(out, node, first);
    // Only these node types can contain semantic children.  Values inside a
    // keyed value/array are represented as parts of their parent entry.
    if (node.type() == DOCUMENT_ROOT || node.type() == OBJECT ||
        node.type() == wasp::FILE)
    {
        for (std::size_t i = 0; i < node.child_count(); ++i)
        {
            walk(out, node.child_at(i), first);
        }
    }
}

std::string trim(const std::string& text)
{
    std::size_t begin = 0;
    while (begin < text.size() && std::isspace(static_cast<unsigned char>(text[begin])))
    {
        ++begin;
    }
    std::size_t end = text.size();
    while (end > begin && std::isspace(static_cast<unsigned char>(text[end - 1])))
    {
        --end;
    }
    return text.substr(begin, end - begin);
}

void emit_diagnostics(std::ostream& out, const std::string& errors)
{
    out << "[";
    std::istringstream lines(errors);
    std::string line;
    bool first = true;
    while (std::getline(lines, line))
    {
        if (line.empty())
        {
            continue;
        }
        if (!first)
        {
            out << ",";
        }
        first = false;
        std::string source;
        std::size_t line_no = 0;
        std::size_t column = 0;
        std::string message = line;
        std::size_t msg_sep = line.rfind(": ");
        if (msg_sep != std::string::npos)
        {
            message = line.substr(msg_sep + 2);
            std::string location = line.substr(0, msg_sep);
            std::size_t loc_sep = location.rfind(':');
            if (loc_sep != std::string::npos)
            {
                source = location.substr(0, loc_sep);
                std::string lc = location.substr(loc_sep + 1);
                std::size_t dot = lc.find('.');
                if (dot != std::string::npos)
                {
                    try
                    {
                        line_no = static_cast<std::size_t>(std::stoul(lc.substr(0, dot)));
                        column = static_cast<std::size_t>(std::stoul(lc.substr(dot + 1)));
                    }
                    catch (...)
                    {
                        line_no = 0;
                        column = 0;
                    }
                }
            }
        }
        out << "{\"source\":" << jstr(source) << ",\"line\":" << line_no
            << ",\"column\":" << column << ",\"message\":" << jstr(trim(message))
            << "}";
    }
    out << "]";
}

int dump_hit(const std::string& path, const std::string& stream_name)
{
    std::ifstream file(path);
    if (!file)
    {
        std::cerr << "cannot open input: " << path << "\n";
        return 2;
    }
    std::stringstream input;
    input << file.rdbuf();

    std::stringstream errors;
    DefaultHITInterpreter interpreter(errors);
    bool ok = interpreter.parseStream(input, stream_name);
    HITNodeView root = interpreter.root();

    std::cout << "{\"suite\":\"hit\",\"parse\":" << (ok ? "true" : "false")
              << ",\"diagnostics\":";
    emit_diagnostics(std::cout, errors.str());
    std::cout << ",\"nodes\":[";
    bool first = true;
    if (!root.is_null())
    {
        walk(std::cout, root, first);
    }
    std::cout << "]}\n";
    return ok ? 0 : 1;
}

int dump_hive(const std::string& schema_path, const std::string& input_path)
{
    std::ifstream schema_file(schema_path);
    std::ifstream input_file(input_path);
    if (!schema_file || !input_file)
    {
        std::cerr << "cannot open schema or input\n";
        return 2;
    }

    std::stringstream schema_stream;
    schema_stream << schema_file.rdbuf();
    std::stringstream input_stream;
    input_stream << input_file.rdbuf();

    std::stringstream schema_errors;
    DefaultSONInterpreter schema_interpreter(schema_errors);
    bool schema_ok =
        schema_interpreter.parseStream(schema_stream, schema_path);

    std::stringstream input_errors;
    DefaultHITInterpreter input_interpreter(input_errors);
    bool input_ok = input_interpreter.parseStream(input_stream, input_path);

    std::vector<std::string> errors;
    bool valid = false;
    if (schema_ok && input_ok)
    {
        HIVE hive;
        SONNodeView schema = schema_interpreter.root();
        HITNodeView input = input_interpreter.root();
        valid = hive.validate(schema, input, errors);
    }
    std::string combined = HIVE::combine(errors);

    std::cout << "{\"suite\":\"hive\",\"schemaParse\":" << (schema_ok ? "true" : "false")
              << ",\"inputParse\":" << (input_ok ? "true" : "false")
              << ",\"valid\":" << (valid ? "true" : "false")
              << ",\"schemaDiagnostics\":" << jstr(schema_errors.str())
              << ",\"inputDiagnostics\":" << jstr(input_errors.str())
              << ",\"combined\":" << jstr(combined) << ",\"errors\":[";
    bool first = true;
    for (const std::string& error : errors)
    {
        if (!first)
        {
            std::cout << ",";
        }
        first = false;
        std::cout << jstr(error);
    }
    std::cout << "]}\n";
    return valid ? 0 : 1;
}

} // namespace

int main(int argc, char** argv)
{
    if (argc < 2)
    {
        std::cerr << "usage: wasp_dump hit <input> [--name NAME]\n"
                  << "       wasp_dump hive <schema> <input>\n";
        return 2;
    }
    std::string command = argv[1];
    if (command == "hit")
    {
        if (argc < 3)
        {
            std::cerr << "usage: wasp_dump hit <input> [--name NAME]\n";
            return 2;
        }
        std::string name = "stream input";
        for (int i = 3; i < argc; ++i)
        {
            if (std::string(argv[i]) == "--name" && i + 1 < argc)
            {
                name = argv[++i];
            }
        }
        return dump_hit(argv[2], name);
    }
    if (command == "hive")
    {
        if (argc < 4)
        {
            std::cerr << "usage: wasp_dump hive <schema> <input>\n";
            return 2;
        }
        return dump_hive(argv[2], argv[3]);
    }
    std::cerr << "unknown command: " << command << "\n";
    return 2;
}
