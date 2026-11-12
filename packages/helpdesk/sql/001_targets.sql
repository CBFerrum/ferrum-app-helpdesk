-- Tickets now keep their first-response and resolution targets on the platform's clock (record_target). Tickets
-- still being worked on when this runs keep the clock they had: it started when they were opened and runs to the
-- moment they were due (response_by, resolution_by), a target already missed stays missed, a response already
-- given stays given, and a ticket waiting for the customer or on hold stays stopped, with the working time it
-- had used estimated from its status history (the time since opening, less the stopped hours it had counted).
-- The seconds each target was given are read from the agreement's priority table. Resolved and closed tickets are
-- left as they were. Nothing here runs on a fresh cluster.
select set_config('ferrum.system', 'on', true);

do $$
begin
  if exists (select 1 from information_schema.columns where table_name = 'e_ticket' and column_name = 'response_secs')
     and exists (select 1 from information_schema.tables where table_name = 'e_service_level_agreement__priorities') then

    update e_ticket t set
      response_secs = round(p.response_hours * 3600)::int,
      resolution_secs = round(p.resolution_hours * 3600)::int,
      sla_pause_hold = coalesce(s.pause_on_hold, false),
      sla_pause_replied = coalesce(s.pause_on_replied, false)
    from e_service_level_agreement s, e_service_level_agreement__priorities p
    where t.status in ('Open', 'Replied', 'On Hold') and t.response_secs is null
      and t.service_level_agreement = s.id and s.tenant_id = t.tenant_id
      and p.parent_id = s.id and p.tenant_id = s.tenant_id and p.priority = t.priority;

    insert into record_target (tenant_id, entity, record_id, target, round, started_at, resumed_at, paused_at, worked_secs, within_secs, due_at, met_at, breached_at)
    select o.tenant_id, 'Ticket', o.id, 'respond', 1, o.opened_at,
           case when o.responded or o.stopped then null else o.opened_at end,
           case when not o.responded and o.stopped and not o.rb then o.last_at end,
           case when o.responded then coalesce(round(o.first_response_hours * 3600), 0)
                when o.stopped then greatest(0, least(o.response_secs, floor(extract(epoch from o.last_at - o.opened_at) - coalesce(o.paused_hours, 0) * 3600)))
                else 0 end,
           o.response_secs,
           case when o.responded or (o.stopped and not o.rb) then o.response_by else coalesce(o.response_by, now()) end,
           case when o.responded then o.first_responded_at end,
           case when o.rb then coalesce(o.first_responded_at, now()) end
    from (select t.*, coalesce((t.status_history -> -1 ->> 'at')::timestamptz, t.opened_at) as last_at,
                 t.first_responded_at is not null as responded,
                 (t.status = 'On Hold' and t.sla_pause_hold) or (t.status = 'Replied' and t.sla_pause_replied) as stopped,
                 coalesce(t.response_breached, false) as rb
          from e_ticket t where t.status in ('Open', 'Replied', 'On Hold') and t.response_secs is not null and t.opened_at is not null) o
    on conflict do nothing;

    insert into record_target (tenant_id, entity, record_id, target, round, started_at, resumed_at, paused_at, worked_secs, within_secs, due_at, met_at, breached_at)
    select o.tenant_id, 'Ticket', o.id, 'resolve', 1, o.opened_at,
           case when o.stopped then null else o.opened_at end,
           case when o.stopped and not o.zb then o.last_at end,
           case when o.stopped then greatest(0, least(o.resolution_secs, floor(extract(epoch from o.last_at - o.opened_at) - coalesce(o.paused_hours, 0) * 3600))) else 0 end,
           o.resolution_secs,
           case when o.stopped and not o.zb then o.resolution_by else coalesce(o.resolution_by, now()) end,
           null,
           case when o.zb then now() end
    from (select t.*, coalesce((t.status_history -> -1 ->> 'at')::timestamptz, t.opened_at) as last_at,
                 (t.status = 'On Hold' and t.sla_pause_hold) or (t.status = 'Replied' and t.sla_pause_replied) as stopped,
                 coalesce(t.resolution_breached, false) as zb
          from e_ticket t where t.status in ('Open', 'Replied', 'On Hold') and t.resolution_secs is not null and t.opened_at is not null) o
    on conflict do nothing;

    update e_ticket t set response_state =
      case when r.met_at is not null then case when r.breached_at is not null then 'met_late' else 'met' end
           when r.breached_at is not null then 'breached' when r.paused_at is not null then 'paused' else 'running' end
    from record_target r
    where r.tenant_id = t.tenant_id and r.entity = 'Ticket' and r.record_id = t.id and r.target = 'respond' and t.response_state is null;

    update e_ticket t set resolution_state =
      case when r.met_at is not null then case when r.breached_at is not null then 'met_late' else 'met' end
           when r.breached_at is not null then 'breached' when r.paused_at is not null then 'paused' else 'running' end
    from record_target r
    where r.tenant_id = t.tenant_id and r.entity = 'Ticket' and r.record_id = t.id and r.target = 'resolve' and t.resolution_state is null;
  end if;
end $$;
