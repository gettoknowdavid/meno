import 'package:flutter_timezone/flutter_timezone.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// A start time the user picked, and the zone they picked it in.
///
/// `start` is **local**: the instant the calendar and the clock showed. Callers
/// hand it to a manager that stores UTC, so the wall clock the user chose is
/// what comes back when it is displayed again. `zone` is the device's IANA
/// name when the platform gave a usable one, because the API stores a zone
/// beside the instant and refuses one that is not a region.
typedef ScheduleChoice = ({DateTime start, String? zone});

/// Asks for a date, then a time, and answers the local instant.
///
/// Two pickers rather than one control: that is the platform's own
/// interaction, and one control for both would be a custom calendar to
/// maintain. A time that is not in the future is refused here with a toast
/// rather than returned, because every caller would otherwise repeat the
/// check.
Future<ScheduleChoice?> pickBroadcastStart(
  BuildContext context, {
  DateTime? initial,
}) async {
  final now = DateTime.now();
  final firstDate = DateUtils.dateOnly(now);
  final lastDate = DateUtils.dateOnly(now.add(const Duration(days: 365)));
  final start = initial?.toLocal() ?? now.add(const Duration(hours: 1));
  final initialDate = DateUtils.dateOnly(start);

  final date = await showDatePicker(
    context: context,
    initialDate: initialDate.isBefore(firstDate)
        ? firstDate
        : initialDate.isAfter(lastDate)
        ? lastDate
        : initialDate,
    firstDate: firstDate,
    lastDate: lastDate,
  );
  if (date == null || !context.mounted) return null;

  final time = await showTimePicker(
    context: context,
    initialTime: TimeOfDay.fromDateTime(start),
  );
  if (time == null || !context.mounted) return null;

  final chosen = DateTime(
    date.year,
    date.month,
    date.day,
    time.hour,
    time.minute,
  );
  if (!chosen.isAfter(DateTime.now())) {
    context.showErrorToast('Choose a start time in the future.');
    return null;
  }

  // The API stores an IANA zone. A failure here leaves the instant exact and
  // merely omits the display zone rather than inventing an abbreviation.
  String? zone;
  try {
    final identifier = (await FlutterTimezone.getLocalTimezone()).identifier;
    zone = _ianaIdentifier(identifier);
  } on Object {
    zone = null;
  }

  return (start: chosen, zone: zone);
}

/// The API requires an IANA name with a region segment.
///
/// The platform may answer `UTC` or a bare abbreviation. The instant is already
/// exact, so an unusable name is omitted rather than sent to be refused; the
/// two no-slash names that are genuinely IANA are folded into their `Etc/`
/// spelling.
String? _ianaIdentifier(String raw) {
  return switch (raw) {
    'UTC' => 'Etc/UTC',
    'GMT' => 'Etc/GMT',
    _ when raw.contains('/') => raw,
    _ => null,
  };
}
