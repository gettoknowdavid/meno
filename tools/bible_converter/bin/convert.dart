import 'package:bible_converter/converter.dart' as converter;
import 'package:bible_converter/seed_writer.dart';

import 'dart:convert';
import 'dart:io';

void main(List<String> arguments) {
  final inputPath = _getFlag(arguments, 'input');
  if (inputPath == null) {
    print('Error: input path is required');
    return;
  }

  final outputPath = _getFlag(arguments, 'output');
  if (outputPath == null) {
    print('Error: output path is required');
    return;
  }

  final code = _getFlag(arguments, 'code');
  if (code == null) {
    print('Error: code is required');
    return;
  }

  // Optional: additionally emit the gzipped payload the mobile app seeds from.
  final seedOutputPath = _getFlag(arguments, 'seed-output');

  final jsonText = File(inputPath).readAsStringSync();
  final jsonDoc = jsonDecode(jsonText) as Map<String, dynamic>;

  final metadata = jsonDoc['metadata'] as Map<String, dynamic>;
  final versesJson = jsonDoc['verses'] as List<dynamic>;

  converter.convertJsonToObjectBox(
    metadata: metadata,
    versesJson: versesJson,
    versionCode: code,
    outputPath: outputPath,
  );

  if (seedOutputPath != null) {
    writeSeedPayload(
      metadata: metadata,
      versesJson: versesJson,
      versionCode: code,
      outputPath: seedOutputPath,
    );
  }
}

String? _getFlag(List<String> args, String name) {
  final index = args.indexOf('--$name');
  if (index == -1) return null;
  final next = args[index + 1];
  if (next.startsWith('--')) return null;
  return next;
}

// String _inferCodeFromPath(String outputPath) {
//   final fileName = File(outputPath).uri.pathSegments.last;
//   return fileName.split('.').first;
// }
