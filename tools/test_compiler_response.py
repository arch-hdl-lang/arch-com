import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from compiler_response import prepare_response, response_schema, verify_response


class CompilerResponseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / 'src').mkdir()
        self.source = self.root / 'src/owners.rs'
        self.source.write_text('pub struct Owner {\nfn parse_owner() {\npub(crate) fn emit_owner() {\n')
        self.rows = [dict(role=role, symbol=symbol, path='src/owners.rs', line=n, quote=line)
                     for n, (role, symbol, line) in enumerate(zip(('ast', 'parser', 'backend'),
                         ('Owner', 'parse_owner', 'emit_owner'), self.source.read_text().splitlines()), 1)]
        self.context = {'status': 'ready', 'payload': '\n'.join(f"src/owners.rs:{r['line']}:{r['quote']}" for r in self.rows),
                        'source_hashes': {'src/owners.rs': hashlib.sha256(self.source.read_bytes()).hexdigest()},
                        'evidence': self.rows}
        self.answer = {r['role']: {k: r[k] for k in ('symbol', 'path', 'line', 'quote')} for r in self.rows}

    def verify(self, answer=None, context=None, **kwargs):
        return verify_response(self.root, context or self.context,
                               json.dumps(self.answer if answer is None else answer), **kwargs)

    def test_source_only_request_and_verified_answer(self):
        request = prepare_response(self.root, self.context, 'Locate owners.')
        self.assertEqual(request['status'], 'ready')
        self.assertEqual(request['validation_read_calls'], 1)
        self.assertNotIn('source_hashes', request['prompt'])
        self.assertEqual(request['output_schema'], response_schema())
        result = self.verify()
        self.assertEqual(result['status'], 'verified')
        self.assertEqual(result['validation_read_bytes'], self.source.stat().st_size)

    def test_stale_source_requests_fallback(self):
        self.source.write_text(self.source.read_text() + '// changed\n')
        self.assertEqual(prepare_response(self.root, self.context, 'Locate owners.')['status'], 'needs_retrieval')
        self.assertEqual(self.verify()['status'], 'needs_retrieval')

    def test_forged_and_duplicate_payload_rejected(self):
        for payload in (self.context['payload'].replace('Owner', 'Invented'), 'src/owners.rs:999:fake',
                        self.context['payload'] + '\n' + self.context['payload'], 'bad row'):
            context = {**self.context, 'payload': payload}
            result = prepare_response(self.root, context, 'Locate owners.')
            self.assertEqual(result['status'], 'needs_retrieval')
            self.assertNotIn('prompt', result)

    def test_null_missing_extra_and_wrong_fields_rejected(self):
        for value in (None, {}, {'symbol': 'Owner'}, {**self.answer['ast'], 'line': True},
                      {**self.answer['ast'], 'line': 0}, {**self.answer['ast'], 'symbol': 'Ow'},
                      {**self.answer['ast'], 'quote': 'pub struct Ow'}, {**self.answer['ast'], 'extra': 1}):
            self.assertEqual(self.verify({**self.answer, 'ast': value})['status'], 'needs_retrieval')
        self.assertEqual(self.verify({k: v for k, v in self.answer.items() if k != 'ast'})['status'], 'needs_retrieval')

    def test_role_swap_and_tool_use_rejected(self):
        swapped = {**self.answer, 'parser': self.answer['backend'], 'backend': self.answer['parser']}
        self.assertEqual(self.verify(swapped)['status'], 'needs_retrieval')
        self.assertEqual(self.verify(used_tools=True)['status'], 'needs_retrieval')

    def test_search_context_supported_without_role_claim(self):
        context = {k: v for k, v in self.context.items() if k != 'evidence'}
        self.assertEqual(self.verify(context=context)['status'], 'verified')
        context['evidence'] = [{k: v for k, v in e.items() if k != 'role'} for e in self.rows]
        self.assertEqual(self.verify(context=context)['status'], 'verified')
        context['evidence'][0]['role'] = 'invalid-role'
        self.assertEqual(self.verify(context=context)['status'], 'needs_retrieval')

    def test_duplicate_json_keys_and_non_json_rejected(self):
        for raw in ('{}', 'null', '[]', '```json\n{}\n```', '{"ast":null,"ast":null}', 'x' * 4097):
            self.assertEqual(verify_response(self.root, self.context, raw)['status'], 'needs_retrieval')

    def test_unavailable_or_oversized_context_has_no_prompt(self):
        for changes in ({'status': 'needs_search'}, {'payload': ''}, {'payload': 'x' * 4097}, {'source_hashes': {}}):
            result = prepare_response(self.root, {**self.context, **changes}, 'Locate owners.')
            self.assertEqual(result['status'], 'needs_retrieval')
            self.assertNotIn('prompt', result)

    def test_symlink_escape_rejected(self):
        outside = self.root / 'outside.rs'
        outside.write_text(self.source.read_text())
        self.source.unlink()
        self.source.symlink_to(outside)
        self.assertEqual(self.verify()['status'], 'needs_retrieval')

    def test_prefix_ending_after_complete_symbol_accepted(self):
        answer = copy.deepcopy(self.answer)
        answer['ast']['quote'] = 'pub struct Owner'
        self.assertEqual(self.verify(answer)['status'], 'verified')

    def test_qualified_graph_binding_accepts_bare_declaration_name(self):
        context = copy.deepcopy(self.context)
        context['evidence'][2]['symbol'] = 'Codegen::emit_owner'
        self.assertEqual(self.verify(context=context)['status'], 'verified')
        wrong = copy.deepcopy(self.answer)
        wrong['backend']['symbol'] = 'emit_other'
        self.assertEqual(self.verify(wrong, context=context)['status'], 'needs_retrieval')

    def test_indented_literal_quote_accepted(self):
        answer = copy.deepcopy(self.answer)
        answer['backend']['quote'] = '    ' + answer['backend']['quote']
        self.assertEqual(self.verify(answer)['status'], 'verified')
        answer['backend']['quote'] = '    ' + 'pub(crate) fn emit_owne'
        self.assertEqual(self.verify(answer)['status'], 'needs_retrieval')

    def test_configuration_validation(self):
        for roles in ((), ('ast', 'ast'), ('unknown',)):
            with self.assertRaises(ValueError):
                response_schema(roles)
        for query in ('', 'x' * 8193):
            with self.assertRaises(ValueError):
                prepare_response(self.root, self.context, query)


if __name__ == '__main__':
    unittest.main()
